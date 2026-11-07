//! Estado do aparelho na memória: a identidade desta execução do app, a agenda de
//! contatos e as etapas do cofre que o app chama pela ponte.
//!
//! Fica no núcleo para a chave nunca atravessar a ponte: o app recebe o código de
//! contato e, no cofre, só o PIN esticado e a chave do Keystore passam por ele.
//!
//! Criar (primeira abertura):
//!   `gerar_identidade` → `preparar_pin_novo` → HMAC no Keystore → `criar_cofre`
//! Abrir (aberturas seguintes):
//!   `preparar_abertura` → HMAC no Keystore → `abrir_cofre`
//! Contatos (só em memória):
//!   `grade_propria` para desenhar o QR; `adicionar_contato_do_quadro` com cada quadro
//!   da câmera (ou `adicionar_contato_por_qr` com o texto já lido).
//!
//! Ordem das travas: a da identidade é solta antes de pegar a da agenda. Nenhuma
//! função segura as duas ao mesmo tempo, então não há como travar uma na outra.

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use zeroize::Zeroizing;

use crate::cartao::CartaoContato;
use crate::codigo::CodigoContato;
use crate::cofre::{self, Custo, ErroCofre, Resultado, TAMANHO_CHAVE, TAMANHO_SAL, Varredura};
use crate::contatos::{Adicao, Agenda, Contato, ErroAgenda};
use crate::identidade::Identidade;
use crate::qr::{self, GradeQr};

static IDENTIDADE: Mutex<Option<Identidade>> = Mutex::new(None);
static AGENDA: Mutex<Agenda> = Mutex::new(Agenda::nova());

fn trava() -> MutexGuard<'static, Option<Identidade>> {
    // Trava envenenada quer dizer que outra thread entrou em pânico segurando-a. O valor
    // guardado continua inteiro, porque toda escrita troca o valor de uma vez. Por isso
    // seguimos com ele em vez de derrubar o app.
    IDENTIDADE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn trava_agenda() -> MutexGuard<'static, Agenda> {
    // Mesmo raciocínio: a agenda só muda por um `push` ou um `clear` inteiros.
    AGENDA.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Gera a identidade do aparelho na primeira chamada e devolve o código de contato.
/// Chamadas seguintes devolvem o mesmo código: tocar duas vezes não troca a chave.
/// Depois de `abrir_cofre`, devolve o código da identidade aberta.
pub fn gerar_identidade() -> CodigoContato {
    let mut guarda = trava();
    guarda.get_or_insert_with(Identidade::nova).codigo_contato()
}

/// Código da identidade na memória, se houver.
pub fn codigo_atual() -> Option<CodigoContato> {
    trava().as_ref().map(Identidade::codigo_contato)
}

/// Tira a identidade e a agenda da memória (a conta do vodozemac zera as chaves ao sair).
/// É o que acontece quando o app fecha; o teste no aparelho usa para simular a reabertura.
pub fn esquecer() {
    *trava() = None;
    trava_agenda().limpar();
}

/// Cartão deste aparelho, para desenhar o QR. `None` sem identidade na memória.
pub fn cartao_proprio() -> Option<CartaoContato> {
    trava()
        .as_ref()
        .map(|i| CartaoContato::de_chaves(&i.chaves_publicas()))
}

/// Põe na agenda o contato do texto lido no QR, conferido por QR.
pub fn adicionar_contato_por_qr(texto: &str) -> Result<Adicao, ErroAgenda> {
    let proprio = trava()
        .as_ref()
        .map(Identidade::chaves_publicas)
        .ok_or(ErroAgenda::SemIdentidade)?;
    trava_agenda().adicionar_por_qr(texto, &proprio)
}

/// Grade do QR deste aparelho, para a tela desenhar. `None` sem identidade na memória.
pub fn grade_propria() -> Result<Option<GradeQr>, qr::ErroQr> {
    cartao_proprio()
        .map(|c| qr::grade_do_cartao(&c))
        .transpose()
}

/// Procura um QR no quadro de brilho da câmera e, se achar, tenta pôr o contato na
/// agenda. `Ok(None)`: o quadro não tem QR legível, e a câmera segue para o próximo.
/// Um QR achado mas recusado (de outro app, adulterado, o próprio) volta como erro.
pub fn adicionar_contato_do_quadro(
    brilho: &[u8],
    largura: usize,
    altura: usize,
    passo: usize,
) -> Result<Option<Adicao>, ErroAgenda> {
    match qr::ler_quadro(brilho, largura, altura, passo)? {
        None => Ok(None),
        Some(texto) => adicionar_contato_por_qr(&texto).map(Some),
    }
}

/// Cópia da agenda, na ordem em que os contatos entraram.
pub fn contatos() -> Vec<Contato> {
    trava_agenda().contatos().to_vec()
}

/// Sal novo e PIN esticado, para o Kotlin passar no HMAC do Keystore.
pub struct PinPreparado {
    pub sal: [u8; TAMANHO_SAL],
    pub esticado: Zeroizing<[u8; TAMANHO_CHAVE]>,
}

pub fn preparar_pin_novo(pin: &[u8]) -> Resultado<PinPreparado> {
    preparar_pin_novo_com_custo(pin, cofre::CUSTO_PADRAO)
}

fn preparar_pin_novo_com_custo(pin: &[u8], custo: Custo) -> Resultado<PinPreparado> {
    cofre::validar_pin(pin)?;
    let sal = cofre::sal_novo()?;
    let esticado = cofre::esticar_pin(pin, &sal, custo)?;
    Ok(PinPreparado { sal, esticado })
}

/// Grava a identidade da memória no cofre novo. Recusa se não há identidade ou se o
/// cofre já existe.
pub fn criar_cofre(pasta: &Path, sal: &[u8], chave_do_cofre: &[u8]) -> Resultado<()> {
    criar_cofre_com_custo(pasta, sal, chave_do_cofre, cofre::CUSTO_PADRAO)
}

fn criar_cofre_com_custo(
    pasta: &Path,
    sal: &[u8],
    chave_do_cofre: &[u8],
    custo: Custo,
) -> Resultado<()> {
    let sal: [u8; TAMANHO_SAL] = sal.try_into().map_err(|_| ErroCofre::ArquivoCorrompido)?;
    let chave = cofre::chave_de(chave_do_cofre)?;
    if cofre::existe(pasta) {
        return Err(ErroCofre::JaExiste);
    }
    let arquivo = {
        let guarda = trava();
        let identidade = guarda.as_ref().ok_or(ErroCofre::SemIdentidade)?;
        cofre::selar(identidade, &chave, sal, custo)?
    };
    cofre::gravar_novo(pasta, &arquivo)
}

/// Espera que a próxima tentativa vai cumprir antes de conferir o PIN.
pub fn espera_atual(pasta: &Path) -> Resultado<Duration> {
    Ok(cofre::espera(cofre::ler_tentativas(pasta)?))
}

/// Primeira etapa da abertura. Registra a tentativa ANTES de conferir (fechar o app
/// no meio não apaga o erro), cumpre a espera e estica o PIN com o sal do arquivo.
/// PIN fora do formato não conta como tentativa.
pub fn preparar_abertura(pasta: &Path, pin: &[u8]) -> Resultado<Zeroizing<[u8; TAMANHO_CHAVE]>> {
    preparar_abertura_com(pasta, pin, thread::sleep)
}

fn preparar_abertura_com(
    pasta: &Path,
    pin: &[u8],
    dormir: impl Fn(Duration),
) -> Resultado<Zeroizing<[u8; TAMANHO_CHAVE]>> {
    cofre::validar_pin(pin)?;
    let arquivo = cofre::ler(pasta)?;
    let (cabecalho, _) = cofre::Cabecalho::ler(&arquivo)?;
    let falhas = cofre::ler_tentativas(pasta)?;
    cofre::gravar_tentativas(pasta, falhas.saturating_add(1))?;
    let espera = cofre::espera(falhas);
    if !espera.is_zero() {
        dormir(espera);
    }
    cofre::esticar_pin(pin, &cabecalho.sal, cabecalho.custo)
}

/// Segunda etapa: abre o cofre com a chave do Keystore. Deu certo: zera as tentativas,
/// põe a identidade na memória e devolve o código.
pub fn abrir_cofre(pasta: &Path, chave_do_cofre: &[u8]) -> Resultado<CodigoContato> {
    let chave = cofre::chave_de(chave_do_cofre)?;
    let arquivo = cofre::ler(pasta)?;
    let identidade = cofre::abrir(&arquivo, &chave)?;
    cofre::gravar_tentativas(pasta, 0)?;
    let codigo = identidade.codigo_contato();
    *trava() = Some(identidade);
    // A agenda foi conferida contra a identidade anterior: começa vazia com a aberta.
    trava_agenda().limpar();
    Ok(codigo)
}

/// Procura as chaves secretas da identidade em memória em todos os arquivos da pasta.
pub fn procurar_segredos(pasta: &Path) -> Resultado<Varredura> {
    let segredos = {
        let guarda = trava();
        let identidade = guarda.as_ref().ok_or(ErroCofre::SemIdentidade)?;
        identidade
            .segredos_para_varredura()
            .map_err(|_| ErroCofre::SemIdentidade)?
    };
    cofre::procurar(pasta, &segredos)
}

#[cfg(test)]
mod testes {
    use std::cell::RefCell;
    use std::fs;

    use super::*;

    const BARATO: Custo = Custo {
        memoria_kib: 64,
        passadas: 1,
        paralelismo: 1,
    };

    /// Faz o papel do Keystore: uma função fixa da entrada, fora do núcleo de verdade.
    fn keystore_falso(esticado: &[u8]) -> Vec<u8> {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(b"keystore falso");
        h.update(esticado);
        h.finalize().to_vec()
    }

    /// Os testes que tocam o estado global (identidade e agenda) pegam esta trava,
    /// para não disputarem entre si quando o `cargo test` roda em paralelo.
    static SERIE: Mutex<()> = Mutex::new(());

    fn em_serie() -> MutexGuard<'static, ()> {
        SERIE.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[test]
    fn ciclo_completo_criar_esquecer_errar_abrir() -> Result<(), Box<dyn std::error::Error>> {
        let _serie = em_serie();
        let pasta = std::env::temp_dir().join(format!("kryptos-aparelho-{}", std::process::id()));
        let _ = fs::remove_dir_all(&pasta);
        fs::create_dir_all(&pasta)?;
        let r = (|| -> Result<(), Box<dyn std::error::Error>> {
            // gerar duas vezes devolve o mesmo código
            let codigo = gerar_identidade();
            assert_eq!(gerar_identidade(), codigo);

            // criar
            let novo = preparar_pin_novo_com_custo(b"135790", BARATO)?;
            let chave = keystore_falso(novo.esticado.as_ref());
            criar_cofre_com_custo(&pasta, &novo.sal, &chave, BARATO)?;
            assert_eq!(
                criar_cofre_com_custo(&pasta, &novo.sal, &chave, BARATO).err(),
                Some(ErroCofre::JaExiste)
            );

            // "fechar o app"
            esquecer();
            assert_eq!(codigo_atual(), None);

            // PIN errado: não abre, conta tentativa, memória continua vazia
            let dormiu = RefCell::new(Vec::new());
            let dormir = |d: Duration| dormiu.borrow_mut().push(d);
            let e = preparar_abertura_com(&pasta, b"000000", dormir)?;
            assert_eq!(
                abrir_cofre(&pasta, &keystore_falso(e.as_ref())).err(),
                Some(ErroCofre::PinIncorreto)
            );
            assert_eq!(codigo_atual(), None);
            assert_eq!(cofre::ler_tentativas(&pasta)?, 1);

            // PIN fora do formato não conta
            assert_eq!(
                preparar_abertura_com(&pasta, b"12", dormir).err(),
                Some(ErroCofre::PinInvalido)
            );
            assert_eq!(cofre::ler_tentativas(&pasta)?, 1);

            // mais quatro erros: a sexta tentativa já espera 30 s
            for _ in 0..4 {
                let e = preparar_abertura_com(&pasta, b"000000", dormir)?;
                assert!(abrir_cofre(&pasta, &keystore_falso(e.as_ref())).is_err());
            }
            assert!(dormiu.borrow().is_empty(), "esperou antes da hora");
            assert_eq!(espera_atual(&pasta)?, Duration::from_secs(30));

            // PIN certo: espera, abre, mesmo código, tentativas zeradas
            let e = preparar_abertura_com(&pasta, b"135790", dormir)?;
            assert_eq!(dormiu.borrow().as_slice(), &[Duration::from_secs(30)]);
            assert_eq!(abrir_cofre(&pasta, &keystore_falso(e.as_ref()))?, codigo);
            assert_eq!(codigo_atual(), Some(codigo));
            assert_eq!(gerar_identidade(), codigo);
            assert_eq!(cofre::ler_tentativas(&pasta)?, 0);

            // chave do Keystore errada (aparelho trocado): não abre
            esquecer();
            let e = preparar_abertura_com(&pasta, b"135790", dormir)?;
            let mut outra = keystore_falso(e.as_ref());
            outra[0] ^= 1;
            assert_eq!(
                abrir_cofre(&pasta, &outra).err(),
                Some(ErroCofre::PinIncorreto)
            );

            // a varredura não acha nada no disco, e precisa de identidade na memória
            assert_eq!(
                procurar_segredos(&pasta).err(),
                Some(ErroCofre::SemIdentidade)
            );
            let e = preparar_abertura_com(&pasta, b"135790", dormir)?;
            abrir_cofre(&pasta, &keystore_falso(e.as_ref()))?;
            let v = procurar_segredos(&pasta)?;
            assert_eq!((v.arquivos, v.ocorrencias, v.ilegiveis), (2, 0, 0));

            // abrir o cofre de novo começa com a agenda vazia
            let outro = Identidade::nova().chaves_publicas();
            adicionar_contato_por_qr(&CartaoContato::de_chaves(&outro).como_texto())?;
            assert_eq!(contatos().len(), 1);
            let e = preparar_abertura_com(&pasta, b"135790", dormir)?;
            abrir_cofre(&pasta, &keystore_falso(e.as_ref()))?;
            assert!(contatos().is_empty(), "agenda sobreviveu à abertura");
            esquecer();
            Ok(())
        })();
        fs::remove_dir_all(&pasta)?;
        r
    }

    #[test]
    fn agenda_segue_a_identidade_da_memoria() -> Result<(), ErroAgenda> {
        let _serie = em_serie();
        esquecer();
        let outro = Identidade::nova();
        let texto = CartaoContato::de_chaves(&outro.chaves_publicas()).como_texto();

        // sem identidade: não há QR próprio e nada entra
        assert!(cartao_proprio().is_none());
        assert_eq!(
            adicionar_contato_por_qr(&texto),
            Err(ErroAgenda::SemIdentidade)
        );
        assert!(contatos().is_empty());

        // com identidade: o QR próprio é o da identidade e é recusado como contato
        let meu_codigo = gerar_identidade();
        let meu = cartao_proprio().ok_or(ErroAgenda::SemIdentidade)?;
        assert_eq!(meu.codigo(), meu_codigo);
        assert_eq!(
            adicionar_contato_por_qr(&meu.como_texto()),
            Err(ErroAgenda::Proprio)
        );

        // o QR do outro entra uma vez só
        assert_eq!(
            adicionar_contato_por_qr(&texto)?,
            Adicao::Novo(outro.codigo_contato())
        );
        assert_eq!(
            adicionar_contato_por_qr(&texto)?,
            Adicao::JaEstava(outro.codigo_contato())
        );
        assert_eq!(contatos().len(), 1);

        // pelo quadro da câmera: sem QR não muda nada; com o QR de outro, entra
        let outro2 = Identidade::nova();
        let grade = qr::grade_do_cartao(&CartaoContato::de_chaves(&outro2.chaves_publicas()))?;
        let (lado, foto) = qr::testes::Foto::nitida(4).desenhar(&grade);
        assert_eq!(
            adicionar_contato_do_quadro(&vec![200u8; lado * lado], lado, lado, lado)?,
            None
        );
        assert_eq!(
            adicionar_contato_do_quadro(&foto, lado, lado, lado)?,
            Some(Adicao::Novo(outro2.codigo_contato()))
        );
        assert_eq!(contatos().len(), 2);
        // o próprio QR pela câmera é recusado
        let (lado, minha_foto) = qr::testes::Foto::nitida(4)
            .desenhar(&grade_propria()?.ok_or(ErroAgenda::SemIdentidade)?);
        assert_eq!(
            adicionar_contato_do_quadro(&minha_foto, lado, lado, lado),
            Err(ErroAgenda::Proprio)
        );
        // quadro de tamanho errado não derruba
        assert_eq!(
            adicionar_contato_do_quadro(&foto, lado + 1, lado, lado + 1),
            Err(ErroAgenda::Quadro(qr::ErroQr::QuadroInvalido))
        );

        // "fechar o app" apaga a agenda junto com a identidade
        esquecer();
        assert!(contatos().is_empty());
        assert!(cartao_proprio().is_none());
        assert_eq!(grade_propria(), Ok(None));
        Ok(())
    }

    #[test]
    fn sem_identidade_nao_cria_cofre_e_abrir_sem_cofre_avisa() -> Result<(), std::io::Error> {
        let pasta = std::env::temp_dir().join(format!("kryptos-vazio-{}", std::process::id()));
        let _ = fs::remove_dir_all(&pasta);
        fs::create_dir_all(&pasta)?;
        assert_eq!(
            preparar_abertura_com(&pasta, b"135790", |_| {}).err(),
            Some(ErroCofre::NaoExiste)
        );
        assert_eq!(
            abrir_cofre(&pasta, &[0u8; 32]).err(),
            Some(ErroCofre::NaoExiste)
        );
        assert_eq!(
            criar_cofre_com_custo(&pasta, &[0u8; 15], &[0u8; 32], BARATO).err(),
            Some(ErroCofre::ArquivoCorrompido),
            "sal de tamanho errado"
        );
        fs::remove_dir_all(&pasta)?;
        Ok(())
    }
}
