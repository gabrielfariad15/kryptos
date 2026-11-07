//! Estado do aparelho na memória: a identidade desta execução do app, a agenda de
//! contatos, as conversas e as etapas do cofre que o app chama pela ponte.
//!
//! Fica no núcleo para a chave nunca atravessar a ponte: o app recebe o código de
//! contato e, no cofre, só o PIN esticado e a chave do Keystore passam por ele.
//!
//! Criar (primeira abertura):
//!   `gerar_identidade` → `preparar_pin_novo` → HMAC no Keystore → `criar_cofre`
//! Abrir (aberturas seguintes):
//!   `preparar_abertura` → HMAC no Keystore → `abrir_cofre`
//! Contatos (gravados no cofre, documentacao/arquitetura/agenda-gravada.md):
//!   `grade_propria` para desenhar o QR; `adicionar_contato_do_quadro` com cada quadro
//!   da câmera (ou `adicionar_contato_por_qr` com o texto já lido). Com o cofre aberto,
//!   o contato só entra na memória depois de a agenda nova estar gravada e lida de
//!   volta. Antes de o cofre existir, fica só na memória e vai junto na criação.
//! Conversas (só em memória; documentacao/arquitetura/sessao-olm.md):
//!   `nova_chave_avulsa` para B publicar; `abrir_conversa` e `cifrar` em A; `receber`
//!   nos dois lados.
//!
//! Com o cofre aberto, a chave do cofre fica na memória junto da conta, para regravar
//! a conta quando ela muda: chave avulsa gerada ou gasta. Regra de ordem: o disco
//! primeiro. Uma chave avulsa só sai daqui depois de gravada e lida de volta; o texto
//! de uma primeira mensagem só sai depois de a chave gasta estar gravada como gasta.
//!
//! Uma trava só: identidade, selo, agenda e conversas pertencem à mesma identidade e
//! mudam juntos.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use vodozemac::olm::OlmMessage;
use zeroize::Zeroizing;

use crate::agenda_gravada;
use crate::cartao::CartaoContato;
use crate::codigo::CodigoContato;
use crate::cofre::{self, Custo, ErroCofre, Resultado, TAMANHO_CHAVE, TAMANHO_SAL, Varredura};
use crate::contatos::{Adicao, Agenda, Contato, ErroAgenda};
use crate::identidade::Identidade;
use crate::qr::{self, GradeQr};
use crate::sessao::{ChaveAvulsa, ErroSessao, Sessao};

/// O que é preciso para regravar o cofre aberto: onde está, a chave que voltou do
/// Keystore e o cabeçalho que não muda (sal e custo). O nonce é novo a cada gravação.
struct Selo {
    pasta: PathBuf,
    chave: Zeroizing<[u8; TAMANHO_CHAVE]>,
    sal: [u8; TAMANHO_SAL],
    custo: Custo,
}

/// Tudo que pertence a uma identidade, sob a mesma trava: a conta, o selo do cofre (se
/// o cofre está aberto), a agenda e as conversas. Trocar de identidade troca tudo de
/// uma vez.
struct Memoria {
    identidade: Identidade,
    selo: Option<Selo>,
    agenda: Agenda,
    /// Uma conversa por contato, a mais recente. Poucos contatos: busca linear.
    conversas: Vec<(CodigoContato, Sessao)>,
}

impl Memoria {
    fn nova(identidade: Identidade, selo: Option<Selo>, agenda: Agenda) -> Self {
        Self {
            identidade,
            selo,
            agenda,
            conversas: Vec::new(),
        }
    }

    fn contato(&self, codigo: CodigoContato) -> Result<Contato, ErroConversa> {
        self.agenda
            .contatos()
            .iter()
            .find(|c| c.codigo() == codigo)
            .copied()
            .ok_or(ErroConversa::ContatoDesconhecido)
    }

    fn conversa(&mut self, codigo: CodigoContato) -> Option<&mut Sessao> {
        self.conversas
            .iter_mut()
            .find(|(c, _)| *c == codigo)
            .map(|(_, s)| s)
    }

    fn guardar_conversa(&mut self, codigo: CodigoContato, sessao: Sessao) {
        self.conversas.retain(|(c, _)| *c != codigo);
        self.conversas.push((codigo, sessao));
    }

    /// Grava a conta da memória no cofre e prova lendo de volta.
    fn regravar(&self) -> Result<(), ErroConversa> {
        let selo = self.selo.as_ref().ok_or(ErroConversa::CofreFechado)?;
        let arquivo = cofre::selar(&self.identidade, &selo.chave, selo.sal, selo.custo)?;
        cofre::regravar(&selo.pasta, &arquivo)?;
        self.conferir_gravado()
    }

    /// O cofre no disco abre com a chave do selo e traz a mesma conta da memória,
    /// chaves avulsas incluídas.
    fn conferir_gravado(&self) -> Result<(), ErroConversa> {
        let selo = self.selo.as_ref().ok_or(ErroConversa::CofreFechado)?;
        let lida = cofre::abrir(&cofre::ler(&selo.pasta)?, &selo.chave)?;
        match lida.mesma_conta_que(&self.identidade) {
            Ok(true) => Ok(()),
            Ok(false) => Err(ErroConversa::Cofre(ErroCofre::ArquivoCorrompido)),
            Err(e) => {
                eprintln!("Kryptos: conta relida do cofre não virou JSON ({e:?})");
                Err(ErroConversa::Cofre(ErroCofre::ArquivoCorrompido))
            }
        }
    }
}

/// Grava a agenda e prova lendo de volta: o arquivo abre com a mesma chave e traz a
/// mesma agenda, na mesma ordem.
fn gravar_agenda(
    pasta: &Path,
    chave: &[u8; TAMANHO_CHAVE],
    agenda: &Agenda,
    proprio: &vodozemac::olm::IdentityKeys,
) -> Resultado<()> {
    cofre::gravar_agenda(pasta, &agenda_gravada::selar(agenda, chave)?)?;
    let lida = cofre::ler_agenda(pasta)?.ok_or(ErroCofre::AgendaCorrompida)?;
    if agenda_gravada::abrir(&lida, chave, proprio)? != *agenda {
        return Err(ErroCofre::AgendaCorrompida);
    }
    Ok(())
}

static IDENTIDADE: Mutex<Option<Memoria>> = Mutex::new(None);

fn trava() -> MutexGuard<'static, Option<Memoria>> {
    // Trava envenenada quer dizer que outra thread entrou em pânico segurando-a. O valor
    // guardado continua inteiro, porque toda escrita troca o valor de uma vez. Por isso
    // seguimos com ele em vez de derrubar o app.
    IDENTIDADE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Motivo de uma operação de conversa não acontecer. Não carrega chave nem texto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroConversa {
    Sessao(ErroSessao),
    Cofre(ErroCofre),
    /// Não há identidade na memória.
    SemIdentidade,
    /// Sem cofre aberto não há onde gravar a chave avulsa: ela se perderia ao fechar.
    CofreFechado,
    /// O código não está na agenda: a chave não foi conferida por QR.
    ContatoDesconhecido,
    /// Mensagem comum de um contato sem conversa aberta (o app fechou, por exemplo).
    SemConversa,
}

impl ErroConversa {
    pub fn codigo(&self) -> &'static str {
        match self {
            Self::Sessao(e) => e.codigo(),
            Self::Cofre(e) => e.codigo(),
            Self::SemIdentidade => "SEM_IDENTIDADE",
            Self::CofreFechado => "COFRE_FECHADO",
            Self::ContatoDesconhecido => "CONTATO_DESCONHECIDO",
            Self::SemConversa => "SEM_CONVERSA",
        }
    }
}

impl fmt::Display for ErroConversa {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErroConversa {}

impl From<ErroSessao> for ErroConversa {
    fn from(e: ErroSessao) -> Self {
        Self::Sessao(e)
    }
}

impl From<ErroCofre> for ErroConversa {
    fn from(e: ErroCofre) -> Self {
        Self::Cofre(e)
    }
}

/// Gera a identidade do aparelho na primeira chamada e devolve o código de contato.
/// Chamadas seguintes devolvem o mesmo código: tocar duas vezes não troca a chave.
/// Depois de `abrir_cofre`, devolve o código da identidade aberta.
pub fn gerar_identidade() -> CodigoContato {
    let mut guarda = trava();
    guarda
        .get_or_insert_with(|| Memoria::nova(Identidade::nova(), None, Agenda::nova()))
        .identidade
        .codigo_contato()
}

/// Código da identidade na memória, se houver.
pub fn codigo_atual() -> Option<CodigoContato> {
    trava().as_ref().map(|m| m.identidade.codigo_contato())
}

/// Tira a identidade, a chave do cofre, as conversas e a agenda da memória (a conta do
/// vodozemac e a chave do cofre zeram ao sair). É o que acontece quando o app fecha; o
/// teste no aparelho usa para simular a reabertura. A agenda gravada fica no disco e
/// volta com `abrir_cofre`.
pub fn esquecer() {
    *trava() = None;
}

/// Cartão deste aparelho, para desenhar o QR. `None` sem identidade na memória.
pub fn cartao_proprio() -> Option<CartaoContato> {
    trava()
        .as_ref()
        .map(|m| CartaoContato::de_chaves(&m.identidade.chaves_publicas()))
}

/// Põe na agenda o contato do texto lido no QR, conferido por QR. Com o cofre aberto,
/// o disco primeiro: a agenda nova é gravada e lida de volta, e só então troca a da
/// memória. Se a gravação falhar, o contato não entra.
pub fn adicionar_contato_por_qr(texto: &str) -> Result<Adicao, ErroAgenda> {
    let mut guarda = trava();
    let memoria = guarda.as_mut().ok_or(ErroAgenda::SemIdentidade)?;
    let proprio = memoria.identidade.chaves_publicas();
    let mut nova = memoria.agenda.clone();
    let adicao = nova.adicionar_por_qr(texto, &proprio)?;
    if let (Adicao::Novo(_), Some(selo)) = (adicao, memoria.selo.as_ref()) {
        gravar_agenda(&selo.pasta, &selo.chave, &nova, &proprio).map_err(ErroAgenda::Gravacao)?;
    }
    memoria.agenda = nova;
    Ok(adicao)
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

/// Cópia da agenda, na ordem em que os contatos entraram. Vazia sem identidade.
pub fn contatos() -> Vec<Contato> {
    trava()
        .as_ref()
        .map(|m| m.agenda.contatos().to_vec())
        .unwrap_or_default()
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

/// Grava a agenda da memória e a identidade no cofre novo e guarda o selo para
/// regravar. Recusa se não há identidade ou se o cofre já existe.
///
/// A agenda vai primeiro, mesmo vazia: assim o arquivo existe desde a criação e a
/// existência dele não diz se há contatos. Se a identidade falhar depois, a agenda que
/// ficou é sobrescrita na próxima tentativa (sem cofre, nada a lê).
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
    let mut guarda = trava();
    let memoria = guarda.as_mut().ok_or(ErroCofre::SemIdentidade)?;
    let proprio = memoria.identidade.chaves_publicas();
    gravar_agenda(pasta, &chave, &memoria.agenda, &proprio)?;
    let arquivo = cofre::selar(&memoria.identidade, &chave, sal, custo)?;
    cofre::gravar_novo(pasta, &arquivo)?;
    memoria.selo = Some(Selo {
        pasta: pasta.to_path_buf(),
        chave,
        sal,
        custo,
    });
    Ok(())
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
/// abre a agenda gravada, põe identidade, selo e agenda na memória e devolve o código.
///
/// Agenda ausente (cofre criado antes da agenda gravada): começa vazia e já é gravada.
/// Agenda que não abre: `AgendaCorrompida`, nada vai para a memória e nada é apagado.
/// O PIN estava certo, então as tentativas já foram zeradas.
pub fn abrir_cofre(pasta: &Path, chave_do_cofre: &[u8]) -> Resultado<CodigoContato> {
    let chave = cofre::chave_de(chave_do_cofre)?;
    let arquivo = cofre::ler(pasta)?;
    let (cabecalho, _) = cofre::Cabecalho::ler(&arquivo)?;
    let identidade = cofre::abrir(&arquivo, &chave)?;
    cofre::gravar_tentativas(pasta, 0)?;
    let proprio = identidade.chaves_publicas();
    let agenda = match cofre::ler_agenda(pasta)? {
        Some(gravada) => agenda_gravada::abrir(&gravada, &chave, &proprio)?,
        None => {
            let vazia = Agenda::nova();
            gravar_agenda(pasta, &chave, &vazia, &proprio)?;
            vazia
        }
    };
    let codigo = identidade.codigo_contato();
    let selo = Selo {
        pasta: pasta.to_path_buf(),
        chave,
        sal: cabecalho.sal,
        custo: cabecalho.custo,
    };
    *trava() = Some(Memoria::nova(identidade, Some(selo), agenda));
    Ok(codigo)
}

/// Procura as chaves secretas da identidade em memória (chaves avulsas incluídas) em
/// todos os arquivos da pasta.
pub fn procurar_segredos(pasta: &Path) -> Resultado<Varredura> {
    let segredos = {
        let guarda = trava();
        let memoria = guarda.as_ref().ok_or(ErroCofre::SemIdentidade)?;
        memoria
            .identidade
            .segredos_para_varredura()
            .map_err(|_| ErroCofre::SemIdentidade)?
    };
    cofre::procurar(pasta, &segredos)
}

// ---------------------------------------------------------------- conversas

/// Gera uma chave avulsa assinada, grava a conta no cofre e só então a entrega. Sem
/// cofre aberto, recusa: a chave se perderia ao fechar o app.
pub fn nova_chave_avulsa() -> Result<ChaveAvulsa, ErroConversa> {
    let mut guarda = trava();
    let memoria = guarda.as_mut().ok_or(ErroConversa::SemIdentidade)?;
    if memoria.selo.is_none() {
        return Err(ErroConversa::CofreFechado);
    }
    let avulsa = memoria.identidade.nova_chave_avulsa()?;
    memoria.regravar()?;
    Ok(avulsa)
}

/// Gera `quantas` chaves avulsas, grava a conta no cofre e só então devolve a
/// publicação assinada para o relay. Com 0, só consulta o estoque. `numero` tem de ser
/// maior que o último que o relay aceitou deste aparelho; se não for, o relay responde
/// `NUMERO_VELHO <último>` e a publicação é refeita com o seguinte.
pub fn preparar_publicacao(quantas: usize, numero: u64) -> Result<Vec<u8>, ErroConversa> {
    let mut guarda = trava();
    let memoria = guarda.as_mut().ok_or(ErroConversa::SemIdentidade)?;
    if memoria.selo.is_none() {
        return Err(ErroConversa::CofreFechado);
    }
    let publicacao = memoria.identidade.publicacao(quantas, numero)?;
    memoria.regravar()?;
    Ok(publicacao)
}

/// A abre a conversa com um contato da agenda, com a chave avulsa dele. Substitui a
/// conversa anterior com o mesmo contato, se houver.
pub fn abrir_conversa(codigo: CodigoContato, avulsa: &ChaveAvulsa) -> Result<(), ErroConversa> {
    let mut guarda = trava();
    let memoria = guarda.as_mut().ok_or(ErroConversa::SemIdentidade)?;
    let contato = memoria.contato(codigo)?;
    let sessao = Sessao::abrir(&memoria.identidade, &contato, avulsa)?;
    memoria.guardar_conversa(codigo, sessao);
    Ok(())
}

/// Cifra para um contato com conversa aberta.
pub fn cifrar(codigo: CodigoContato, texto: &[u8]) -> Result<OlmMessage, ErroConversa> {
    let mut guarda = trava();
    let memoria = guarda.as_mut().ok_or(ErroConversa::SemIdentidade)?;
    let sessao = memoria.conversa(codigo).ok_or(ErroConversa::SemConversa)?;
    Ok(sessao.cifrar(texto)?)
}

/// Recebe uma mensagem de um contato da agenda.
///
/// - Primeira mensagem da conversa que já está aberta (A mandou várias antes de B
///   responder): decifra nela.
/// - Primeira mensagem de uma conversa nova: abre a conversa, grava a conta com a chave
///   avulsa gasta e só então devolve o texto. Se a gravação falhar, a conversa é
///   descartada e o texto não sai: depois de reabrir o app, a mesma mensagem ainda
///   abre, porque o cofre continua com a chave.
/// - Mensagem comum: decifra na conversa aberta.
pub fn receber(
    codigo: CodigoContato,
    mensagem: &OlmMessage,
) -> Result<Zeroizing<Vec<u8>>, ErroConversa> {
    let mut guarda = trava();
    let memoria = guarda.as_mut().ok_or(ErroConversa::SemIdentidade)?;
    let contato = memoria.contato(codigo)?;
    if let OlmMessage::PreKey(primeira) = mensagem {
        let ja_aberta = memoria
            .conversa(codigo)
            .filter(|s| s.identificador() == primeira.session_id());
        if let Some(sessao) = ja_aberta {
            return Ok(sessao.decifrar(mensagem)?);
        }
        if memoria.selo.is_none() {
            return Err(ErroConversa::CofreFechado);
        }
        let (sessao, texto) =
            Sessao::receber_primeira(&mut memoria.identidade, &contato, mensagem)?;
        memoria.regravar()?;
        memoria.guardar_conversa(codigo, sessao);
        return Ok(texto);
    }
    let sessao = memoria.conversa(codigo).ok_or(ErroConversa::SemConversa)?;
    Ok(sessao.decifrar(mensagem)?)
}

#[cfg(test)]
mod testes {
    use std::cell::RefCell;
    use std::fs;

    use super::*;
    use crate::contatos::Agenda;

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
            assert_eq!((v.arquivos, v.ocorrencias, v.ilegiveis), (3, 0, 0));

            // a agenda volta ao abrir o cofre de novo
            let outro = Identidade::nova().chaves_publicas();
            adicionar_contato_por_qr(&CartaoContato::de_chaves(&outro).como_texto())?;
            assert_eq!(contatos().len(), 1);
            let antes = contatos();
            esquecer();
            assert!(contatos().is_empty());
            let e = preparar_abertura_com(&pasta, b"135790", dormir)?;
            abrir_cofre(&pasta, &keystore_falso(e.as_ref()))?;
            assert_eq!(contatos(), antes, "agenda não voltou ao abrir");
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
        // quadro de tamanho errado não derruba (lado e quadro do mesmo desenho: o QR de
        // cerca de 1 em 100 cartões sai na versão 6, com outro lado)
        assert_eq!(
            adicionar_contato_do_quadro(&minha_foto, lado + 1, lado, lado + 1),
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

    // ---------- conversas e chaves avulsas no cofre ----------

    const PIN: &[u8] = b"246801";

    fn pasta_de_teste(nome: &str) -> Result<PathBuf, std::io::Error> {
        let pasta = std::env::temp_dir().join(format!("kryptos-{nome}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&pasta);
        fs::create_dir_all(&pasta)?;
        Ok(pasta)
    }

    /// B: identidade nova na memória e cofre criado com custo baixo.
    fn b_com_cofre(pasta: &Path) -> Result<CodigoContato, Box<dyn std::error::Error>> {
        esquecer();
        let codigo = gerar_identidade();
        let novo = preparar_pin_novo_com_custo(PIN, BARATO)?;
        criar_cofre_com_custo(
            pasta,
            &novo.sal,
            &keystore_falso(novo.esticado.as_ref()),
            BARATO,
        )?;
        Ok(codigo)
    }

    /// "Fecha e abre o app": tira tudo da memória e abre o cofre de novo. Devolve o
    /// código aberto. A agenda volta do disco; as conversas, não.
    fn reabrir(pasta: &Path) -> Result<CodigoContato, Box<dyn std::error::Error>> {
        esquecer();
        let e = preparar_abertura_com(pasta, PIN, |_| {})?;
        Ok(abrir_cofre(pasta, &keystore_falso(e.as_ref()))?)
    }

    /// A, fora do aparelho global: identidade própria, B na agenda dela pelo QR de B.
    fn a_que_leu_o_qr_de_b() -> Result<(Identidade, Contato), Box<dyn std::error::Error>> {
        let a = Identidade::nova();
        let mut agenda = Agenda::nova();
        let qr_de_b = cartao_proprio().ok_or("sem identidade")?.como_texto();
        agenda.adicionar_por_qr(&qr_de_b, &a.chaves_publicas())?;
        Ok((a, agenda.contatos()[0]))
    }

    /// B lê o QR de A.
    fn b_le_o_qr_de(a: &Identidade) -> Result<CodigoContato, Box<dyn std::error::Error>> {
        adicionar_contato_por_qr(&CartaoContato::de_chaves(&a.chaves_publicas()).como_texto())?;
        Ok(a.codigo_contato())
    }

    fn com_pasta(
        nome: &str,
        teste: impl FnOnce(&Path) -> Result<(), Box<dyn std::error::Error>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let _serie = em_serie();
        let pasta = pasta_de_teste(nome)?;
        let r = teste(&pasta);
        esquecer();
        fs::remove_dir_all(&pasta)?;
        r
    }

    #[test]
    fn chave_avulsa_sobrevive_a_fechar_e_a_gasta_continua_gasta()
    -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("avulsa", |pasta| {
            let codigo_b = b_com_cofre(pasta)?;
            let avulsa = nova_chave_avulsa()?;
            let (a, b_para_a) = a_que_leu_o_qr_de_b()?;
            let mut sa = Sessao::abrir(&a, &b_para_a, &avulsa)?;
            let m1 = sa.cifrar(b"um")?;
            let m2 = sa.cifrar(b"dois")?;

            // B fecha o app antes de a mensagem chegar.
            assert_eq!(reabrir(pasta)?, codigo_b);
            let codigo_a = b_le_o_qr_de(&a)?;
            assert_eq!(receber(codigo_a, &m1)?.as_slice(), b"um");
            assert_eq!(
                receber(codigo_a, &m2)?.as_slice(),
                b"dois",
                "segunda primeira"
            );
            assert_eq!(
                receber(codigo_a, &m1).err(),
                Some(ErroConversa::Sessao(ErroSessao::JaUsada)),
                "repetida com a conversa aberta"
            );
            let resposta = cifrar(codigo_a, b"oi, A")?;
            assert_eq!(sa.decifrar(&resposta)?.as_slice(), b"oi, A");
            let m3 = sa.cifrar(b"tres")?;
            assert_eq!(receber(codigo_a, &m3)?.as_slice(), b"tres");

            // Fecha de novo: a chave avulsa gasta continua gasta, e a primeira mensagem
            // repetida não abre outra conversa (nem mostra o texto de novo).
            reabrir(pasta)?;
            let codigo_a = b_le_o_qr_de(&a)?;
            assert_eq!(
                receber(codigo_a, &m1).err(),
                Some(ErroConversa::Sessao(ErroSessao::ChaveAvulsaDesconhecida))
            );
            // A conversa não sobrevive a fechar o app (fica só em memória).
            let m4 = sa.cifrar(b"quatro")?;
            assert_eq!(
                receber(codigo_a, &m4).err(),
                Some(ErroConversa::SemConversa)
            );
            Ok(())
        })
    }

    /// Varredura calibrada (regra 5.3): com chaves avulsas na conta, o cofre não tem
    /// nenhum segredo em claro; a mesma conta gravada sem cifra é achada.
    #[test]
    fn varredura_nao_acha_chave_avulsa_no_cofre() -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("varredura-avulsa", |pasta| {
            b_com_cofre(pasta)?;
            for _ in 0..3 {
                nova_chave_avulsa()?;
            }
            // cofre, tentativas e agenda
            let v = procurar_segredos(pasta)?;
            assert_eq!((v.arquivos, v.ocorrencias, v.ilegiveis), (3, 0, 0));
            let em_claro = trava()
                .as_ref()
                .ok_or("sem identidade")?
                .identidade
                .pickle_em_claro_para_teste()?;
            fs::write(pasta.join("vazamento.json"), em_claro.as_slice())?;
            let v = procurar_segredos(pasta)?;
            assert!(v.ocorrencias >= 5, "achou {} de 5 segredos", v.ocorrencias);
            Ok(())
        })
    }

    #[test]
    fn sem_cofre_nao_entrega_nem_gasta_chave_avulsa() -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("sem-cofre", |pasta| {
            esquecer();
            assert_eq!(nova_chave_avulsa().err(), Some(ErroConversa::SemIdentidade));
            gerar_identidade();
            assert_eq!(nova_chave_avulsa().err(), Some(ErroConversa::CofreFechado));
            // Sem cofre, B também não gasta chave avulsa ao receber: não teria onde
            // gravar que gastou. A chave é gerada por fora da regra só para o teste.
            let avulsa = trava()
                .as_mut()
                .ok_or("sem identidade")?
                .identidade
                .nova_chave_avulsa()?;
            let (a, b_para_a) = a_que_leu_o_qr_de_b()?;
            let m = Sessao::abrir(&a, &b_para_a, &avulsa)?.cifrar(b"x")?;
            let codigo_a = b_le_o_qr_de(&a)?;
            assert_eq!(
                receber(codigo_a, &m).err(),
                Some(ErroConversa::CofreFechado)
            );
            // A recusa não gastou a chave: com o cofre criado, a mesma mensagem abre.
            let novo = preparar_pin_novo_com_custo(PIN, BARATO)?;
            criar_cofre_com_custo(
                pasta,
                &novo.sal,
                &keystore_falso(novo.esticado.as_ref()),
                BARATO,
            )?;
            assert_eq!(receber(codigo_a, &m)?.as_slice(), b"x");
            cofre::apagar(pasta)?;

            // Cofre apagado com o app aberto: não entrega chave nova.
            b_com_cofre(pasta)?;
            let avulsa = nova_chave_avulsa()?;
            let (a, b_para_a) = a_que_leu_o_qr_de_b()?;
            let m1 = Sessao::abrir(&a, &b_para_a, &avulsa)?.cifrar(b"um")?;
            let codigo_a = b_le_o_qr_de(&a)?;
            let arquivo = cofre::ler(pasta)?;
            cofre::apagar(pasta)?;
            assert_eq!(
                nova_chave_avulsa().err(),
                Some(ErroConversa::Cofre(ErroCofre::NaoExiste))
            );
            // E a primeira mensagem não sai sem a chave gasta gravada.
            assert_eq!(
                receber(codigo_a, &m1).err(),
                Some(ErroConversa::Cofre(ErroCofre::NaoExiste))
            );
            // O cofre de antes volta (como depois de uma falha passageira de disco):
            // reaberto, a mesma mensagem abre, porque a chave não foi gravada como gasta.
            cofre::gravar_novo(pasta, &arquivo)?;
            reabrir(pasta)?;
            let codigo_a = b_le_o_qr_de(&a)?;
            assert_eq!(receber(codigo_a, &m1)?.as_slice(), b"um");
            Ok(())
        })
    }

    #[test]
    fn contato_fora_da_agenda_e_conversa_fechada_sao_recusados()
    -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("fora-da-agenda", |pasta| {
            b_com_cofre(pasta)?;
            let avulsa = nova_chave_avulsa()?;
            let (a, b_para_a) = a_que_leu_o_qr_de_b()?;
            let m1 = Sessao::abrir(&a, &b_para_a, &avulsa)?.cifrar(b"um")?;
            let estranho = Identidade::nova().codigo_contato();
            assert_eq!(
                receber(a.codigo_contato(), &m1).err(),
                Some(ErroConversa::ContatoDesconhecido),
                "A ainda não está na agenda de B"
            );
            assert_eq!(
                abrir_conversa(estranho, &avulsa).err(),
                Some(ErroConversa::ContatoDesconhecido)
            );
            let codigo_a = b_le_o_qr_de(&a)?;
            assert_eq!(
                cifrar(codigo_a, b"x").err(),
                Some(ErroConversa::SemConversa)
            );
            // A chave não foi gasta pela recusa: a mesma mensagem abre agora.
            assert_eq!(receber(codigo_a, &m1)?.as_slice(), b"um");
            Ok(())
        })
    }

    /// B abre a conversa (B escreve primeiro para A) pelo aparelho.
    #[test]
    fn b_tambem_abre_conversa_pelo_aparelho() -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("abrir-conversa", |pasta| {
            b_com_cofre(pasta)?;
            let (mut a, a_ve_b) = a_que_leu_o_qr_de_b()?;
            let codigo_a = b_le_o_qr_de(&a)?;
            let avulsa_de_a = a.nova_chave_avulsa()?;
            abrir_conversa(codigo_a, &avulsa_de_a)?;
            let m = cifrar(codigo_a, b"de B")?;
            let (_, lido) = Sessao::receber_primeira(&mut a, &a_ve_b, &m)?;
            assert_eq!(lido.as_slice(), b"de B");
            Ok(())
        })
    }

    /// Calibração da releitura (regra 5.3): o cofre com a conta da memória confere; com
    /// uma chave avulsa a menos, ou com outra conta aberta pela mesma chave, não.
    #[test]
    fn releitura_recusa_cofre_com_outra_conta() -> Result<(), Box<dyn std::error::Error>> {
        let pasta = pasta_de_teste("releitura")?;
        let r = (|| -> Result<(), Box<dyn std::error::Error>> {
            let chave = cofre::chave_de(&[5u8; 32])?;
            let sal = cofre::sal_novo()?;
            let mut m = Memoria::nova(Identidade::nova(), None, Agenda::nova());
            cofre::gravar_novo(&pasta, &cofre::selar(&m.identidade, &chave, sal, BARATO)?)?;
            m.selo = Some(Selo {
                pasta: pasta.clone(),
                chave: cofre::chave_de(&[5u8; 32])?,
                sal,
                custo: BARATO,
            });
            m.conferir_gravado()?;
            let antes = cofre::ler(&pasta)?;
            // Uma chave avulsa a mais na memória do que no disco.
            m.identidade.nova_chave_avulsa()?;
            assert_eq!(
                m.conferir_gravado().err(),
                Some(ErroConversa::Cofre(ErroCofre::ArquivoCorrompido))
            );
            m.regravar()?;
            m.conferir_gravado()?;
            // Volta o arquivo de antes: falta a chave avulsa.
            cofre::regravar(&pasta, &antes)?;
            assert!(m.conferir_gravado().is_err());
            // Outra conta, cifrada com a mesma chave.
            let outra = Identidade::nova();
            cofre::regravar(&pasta, &cofre::selar(&outra, &chave, sal, BARATO)?)?;
            assert_eq!(
                m.conferir_gravado().err(),
                Some(ErroConversa::Cofre(ErroCofre::ArquivoCorrompido))
            );
            Ok(())
        })();
        fs::remove_dir_all(&pasta)?;
        r
    }

    /// As chaves da publicação sobrevivem a fechar o app antes de alguém retirá-las.
    #[test]
    fn chaves_publicadas_sobrevivem_a_fechar() -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("publicacao", |pasta| {
            esquecer();
            gerar_identidade();
            assert_eq!(
                preparar_publicacao(1, 1).err(),
                Some(ErroConversa::CofreFechado)
            );
            b_com_cofre(pasta)?;
            let bytes = preparar_publicacao(2, 1)?;
            let publicada = kryptos_protocolo::Publicacao::conferir(&bytes)?;
            assert_eq!(publicada.chaves.len(), 2);
            reabrir(pasta)?;
            let (a, b_para_a) = a_que_leu_o_qr_de_b()?;
            let codigo_a = b_le_o_qr_de(&a)?;
            for (i, k) in publicada.chaves.iter().enumerate() {
                let avulsa = ChaveAvulsa::da_retirada(&k.para_bytes())?;
                let texto = format!("mensagem {i}");
                let m = Sessao::abrir(&a, &b_para_a, &avulsa)?.cifrar(texto.as_bytes())?;
                assert_eq!(receber(codigo_a, &m)?.as_slice(), texto.as_bytes());
            }
            Ok(())
        })
    }

    #[test]
    fn codigos_de_erro_da_conversa_sao_distintos() {
        let todos = [
            ErroConversa::SemIdentidade,
            ErroConversa::CofreFechado,
            ErroConversa::ContatoDesconhecido,
            ErroConversa::SemConversa,
            ErroConversa::Sessao(ErroSessao::JaUsada),
            ErroConversa::Cofre(ErroCofre::NaoExiste),
        ]
        .map(|e| e.codigo());
        let mut unicos = todos.to_vec();
        unicos.sort_unstable();
        unicos.dedup();
        assert_eq!(unicos.len(), todos.len());
    }

    // ---------- agenda gravada ----------

    fn texto_de_outro() -> (CodigoContato, String) {
        let outro = Identidade::nova();
        (
            outro.codigo_contato(),
            CartaoContato::de_chaves(&outro.chaves_publicas()).como_texto(),
        )
    }

    fn bytes_da_agenda(pasta: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        Ok(cofre::ler_agenda(pasta)?.ok_or("sem agenda no disco")?)
    }

    #[test]
    fn agenda_gravada_volta_depois_de_fechar_e_some_com_apagar()
    -> Result<(), Box<dyn std::error::Error>> {
        let _serie = em_serie();
        let pasta = pasta_de_teste("agenda-volta")?;
        let r = (|| -> Result<(), Box<dyn std::error::Error>> {
            // contato lido antes de o cofre existir vai junto na criação
            esquecer();
            gerar_identidade();
            let (primeiro, texto1) = texto_de_outro();
            assert_eq!(adicionar_contato_por_qr(&texto1)?, Adicao::Novo(primeiro));
            assert!(
                cofre::ler_agenda(&pasta)?.is_none(),
                "gravou antes do cofre"
            );
            let novo = preparar_pin_novo_com_custo(PIN, BARATO)?;
            criar_cofre_com_custo(
                &pasta,
                &novo.sal,
                &keystore_falso(novo.esticado.as_ref()),
                BARATO,
            )?;
            let na_criacao = bytes_da_agenda(&pasta)?;
            assert_eq!(na_criacao.len(), agenda_gravada::TAMANHO_ARQUIVO);

            // contato novo com o cofre aberto: arquivo regravado, mesmo tamanho
            let (segundo, texto2) = texto_de_outro();
            assert_eq!(adicionar_contato_por_qr(&texto2)?, Adicao::Novo(segundo));
            let depois = bytes_da_agenda(&pasta)?;
            assert_ne!(depois, na_criacao);
            assert_eq!(depois.len(), agenda_gravada::TAMANHO_ARQUIVO);

            // o mesmo QR de novo não regrava
            assert_eq!(
                adicionar_contato_por_qr(&texto2)?,
                Adicao::JaEstava(segundo)
            );
            assert_eq!(bytes_da_agenda(&pasta)?, depois);

            // fecha e abre: os dois voltam, na mesma ordem
            let antes = contatos();
            reabrir(&pasta)?;
            assert_eq!(contatos(), antes);
            assert_eq!(
                contatos().iter().map(|c| c.codigo()).collect::<Vec<_>>(),
                vec![primeiro, segundo]
            );

            // apagar leva a agenda junto
            esquecer();
            cofre::apagar(&pasta)?;
            assert!(cofre::ler_agenda(&pasta)?.is_none());
            assert_eq!(fs::read_dir(&pasta)?.count(), 0);
            Ok(())
        })();
        esquecer();
        fs::remove_dir_all(&pasta)?;
        r
    }

    #[test]
    fn cofre_sem_agenda_abre_com_agenda_vazia_ja_gravada() -> Result<(), Box<dyn std::error::Error>>
    {
        let _serie = em_serie();
        let pasta = pasta_de_teste("agenda-ausente")?;
        let r = (|| -> Result<(), Box<dyn std::error::Error>> {
            b_com_cofre(&pasta)?;
            // cofre criado antes da agenda gravada existir
            fs::remove_file(pasta.join(cofre::NOME_AGENDA))?;
            reabrir(&pasta)?;
            assert!(contatos().is_empty());
            assert_eq!(
                bytes_da_agenda(&pasta)?.len(),
                agenda_gravada::TAMANHO_ARQUIVO
            );
            Ok(())
        })();
        esquecer();
        fs::remove_dir_all(&pasta)?;
        r
    }

    #[test]
    fn agenda_adulterada_nao_abre_e_nada_e_apagado() -> Result<(), Box<dyn std::error::Error>> {
        let _serie = em_serie();
        let pasta = pasta_de_teste("agenda-adulterada")?;
        let r = (|| -> Result<(), Box<dyn std::error::Error>> {
            b_com_cofre(&pasta)?;
            adicionar_contato_por_qr(&texto_de_outro().1)?;
            let mut agenda = bytes_da_agenda(&pasta)?;
            let meio = agenda.len() / 2;
            agenda[meio] ^= 0x80;
            fs::write(pasta.join(cofre::NOME_AGENDA), &agenda)?;
            let cofre_antes = cofre::ler(&pasta)?;

            esquecer();
            let e = preparar_abertura_com(&pasta, PIN, |_| {})?;
            assert_eq!(
                abrir_cofre(&pasta, &keystore_falso(e.as_ref())).err(),
                Some(ErroCofre::AgendaCorrompida)
            );
            assert_eq!(codigo_atual(), None, "abriu pela metade");
            assert!(contatos().is_empty());
            assert_eq!(bytes_da_agenda(&pasta)?, agenda, "mexeu na agenda");
            assert_eq!(cofre::ler(&pasta)?, cofre_antes, "mexeu no cofre");
            // o PIN estava certo: não fica contando como erro
            assert_eq!(cofre::ler_tentativas(&pasta)?, 0);
            Ok(())
        })();
        esquecer();
        fs::remove_dir_all(&pasta)?;
        r
    }

    #[test]
    fn gravacao_da_agenda_falha_e_o_contato_nao_entra() -> Result<(), Box<dyn std::error::Error>> {
        let _serie = em_serie();
        let pasta = pasta_de_teste("agenda-falha")?;
        let r = (|| -> Result<(), Box<dyn std::error::Error>> {
            b_com_cofre(&pasta)?;
            let gravada = bytes_da_agenda(&pasta)?;
            // uma pasta no lugar do arquivo: a troca de nome falha
            fs::remove_file(pasta.join(cofre::NOME_AGENDA))?;
            fs::create_dir(pasta.join(cofre::NOME_AGENDA))?;
            let (codigo, texto) = texto_de_outro();
            assert!(matches!(
                adicionar_contato_por_qr(&texto),
                Err(ErroAgenda::Gravacao(ErroCofre::Disco(_)))
            ));
            assert!(contatos().is_empty(), "entrou sem gravar");
            assert!(
                !pasta.join(format!("{}.novo", cofre::NOME_AGENDA)).exists(),
                "temporário ficou no disco"
            );
            // disco de volta: o mesmo QR entra e grava
            fs::remove_dir(pasta.join(cofre::NOME_AGENDA))?;
            fs::write(pasta.join(cofre::NOME_AGENDA), &gravada)?;
            assert_eq!(adicionar_contato_por_qr(&texto)?, Adicao::Novo(codigo));
            reabrir(&pasta)?;
            assert_eq!(
                contatos().iter().map(|c| c.codigo()).collect::<Vec<_>>(),
                vec![codigo]
            );
            Ok(())
        })();
        esquecer();
        fs::remove_dir_all(&pasta)?;
        r
    }
}
