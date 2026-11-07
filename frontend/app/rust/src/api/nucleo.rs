//! Funções do núcleo expostas ao app. Cada uma só repassa para `kryptos_nucleo`.
//!
//! Erros chegam ao Dart como `AnyhowException` com o código fixo do núcleo
//! (`PIN_INCORRETO`, `COFRE_CORROMPIDO`...): nada de PIN, chave ou código de contato.

use std::path::Path;

use kryptos_nucleo::aparelho;
use kryptos_nucleo::cofre::ErroCofre;
use kryptos_nucleo::qr;
use kryptos_nucleo::{Adicao, Conferencia};
use zeroize::Zeroizing;

fn erro(e: ErroCofre) -> anyhow::Error {
    anyhow::anyhow!(e.to_string())
}

/// Versão do núcleo compilado dentro do app.
#[flutter_rust_bridge::frb(sync)]
pub fn versao_nucleo() -> String {
    kryptos_nucleo::versao().to_string()
}

/// Gera a identidade do aparelho (uma vez por execução) e devolve o código de
/// contato no formato XXXX-XXXX. A chave fica no núcleo; só o código atravessa.
#[flutter_rust_bridge::frb(sync)]
pub fn gerar_codigo_contato() -> String {
    aparelho::gerar_identidade().como_texto()
}

/// Código da identidade que está na memória, ou nada.
#[flutter_rust_bridge::frb(sync)]
pub fn codigo_atual() -> Option<String> {
    aparelho::codigo_atual().map(|c| c.como_texto())
}

/// Tira a identidade da memória (como fechar o app).
#[flutter_rust_bridge::frb(sync)]
pub fn esquecer_identidade() {
    aparelho::esquecer();
}

#[flutter_rust_bridge::frb(sync)]
pub fn cofre_existe(pasta: String) -> bool {
    kryptos_nucleo::cofre::existe(Path::new(&pasta))
}

/// Segundos que a próxima tentativa de PIN vai esperar antes de conferir.
#[flutter_rust_bridge::frb(sync)]
pub fn cofre_espera_segundos(pasta: String) -> anyhow::Result<u32> {
    aparelho::espera_atual(Path::new(&pasta))
        .map(|d| u32::try_from(d.as_secs()).unwrap_or(u32::MAX))
        .map_err(erro)
}

/// Sal novo e PIN esticado (Argon2id), para o HMAC do Keystore.
pub struct PinPreparado {
    pub sal: Vec<u8>,
    pub esticado: Vec<u8>,
}

/// Criar, etapa 1. O PIN chega como dígitos ASCII.
pub fn cofre_preparar_pin_novo(pin: Vec<u8>) -> anyhow::Result<PinPreparado> {
    let pin = Zeroizing::new(pin);
    let p = aparelho::preparar_pin_novo(&pin).map_err(erro)?;
    Ok(PinPreparado {
        sal: p.sal.to_vec(),
        esticado: p.esticado.to_vec(),
    })
}

/// Criar, etapa 2: grava a identidade da memória com a chave que voltou do Keystore.
pub fn cofre_criar(pasta: String, sal: Vec<u8>, chave: Vec<u8>) -> anyhow::Result<()> {
    let chave = Zeroizing::new(chave);
    aparelho::criar_cofre(Path::new(&pasta), &sal, &chave).map_err(erro)
}

/// Abrir, etapa 1: registra a tentativa, cumpre a espera e estica o PIN.
pub fn cofre_preparar_abertura(pasta: String, pin: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    let pin = Zeroizing::new(pin);
    aparelho::preparar_abertura(Path::new(&pasta), &pin)
        .map(|e| e.to_vec())
        .map_err(erro)
}

/// Abrir, etapa 2: abre com a chave do Keystore e devolve o código de contato.
pub fn cofre_abrir(pasta: String, chave: Vec<u8>) -> anyhow::Result<String> {
    let chave = Zeroizing::new(chave);
    aparelho::abrir_cofre(Path::new(&pasta), &chave)
        .map(|c| c.como_texto())
        .map_err(erro)
}

/// Apaga o arquivo do cofre e o contador (a chave do Keystore o Kotlin apaga).
pub fn cofre_apagar(pasta: String) -> anyhow::Result<()> {
    kryptos_nucleo::cofre::apagar(Path::new(&pasta)).map_err(erro)
}

/// Contagens da busca pelas chaves secretas em disco. Nunca o conteúdo.
pub struct Varredura {
    pub arquivos: u32,
    pub ocorrencias: u32,
    pub ilegiveis: u32,
}

/// Procura as chaves secretas da identidade aberta em todos os arquivos da pasta.
pub fn procurar_segredos(pasta: String) -> anyhow::Result<Varredura> {
    let v = aparelho::procurar_segredos(Path::new(&pasta)).map_err(erro)?;
    Ok(Varredura {
        arquivos: v.arquivos,
        ocorrencias: v.ocorrencias,
        ilegiveis: v.ilegiveis,
    })
}

// ---------- Contatos por QR (documentacao/arquitetura/cartao-de-contato.md) ----------
//
// Só atravessam a ponte a grade do QR próprio (chave pública desenhada), o código de
// contato e a forma como a chave foi conferida. O texto do cartão e as chaves ficam
// no núcleo. Erro chega como o código fixo do núcleo (QR_PROPRIO, QR_NAO_CONFERE...).

/// Grade do QR deste aparelho: `lado` × `lado` módulos, linha por linha, 1 = escuro.
/// A tela desenha a margem clara de `margem` módulos em volta.
pub struct GradeDoQr {
    pub lado: u32,
    pub margem: u32,
    pub modulos: Vec<u8>,
}

/// Grade do QR da identidade na memória, ou nada se não há identidade.
#[flutter_rust_bridge::frb(sync)]
pub fn grade_propria() -> anyhow::Result<Option<GradeDoQr>> {
    let grade = aparelho::grade_propria().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    grade
        .map(|g| {
            Ok(GradeDoQr {
                lado: u32::try_from(g.lado()).map_err(|_| anyhow::anyhow!("QR_GERACAO"))?,
                margem: u32::try_from(qr::MARGEM_MODULOS)
                    .map_err(|_| anyhow::anyhow!("QR_GERACAO"))?,
                modulos: g.modulos().iter().map(|&escuro| u8::from(escuro)).collect(),
            })
        })
        .transpose()
}

/// Contato achado num quadro da câmera.
pub struct ContatoLido {
    pub codigo: String,
    /// `false`: já estava na agenda com as mesmas chaves.
    pub novo: bool,
}

/// Procura um QR no plano de brilho (Y) de um quadro da câmera e, se achar um cartão
/// aceito, põe o contato na agenda. Nada: o quadro não tem QR legível.
/// `passo` é o `bytesPerRow` do plano, que pode passar da largura.
pub fn adicionar_contato_do_quadro(
    brilho: Vec<u8>,
    largura: u32,
    altura: u32,
    passo: u32,
) -> anyhow::Result<Option<ContatoLido>> {
    let medida = |v: u32| usize::try_from(v).map_err(|_| anyhow::anyhow!("QUADRO_INVALIDO"));
    let adicao = aparelho::adicionar_contato_do_quadro(
        &brilho,
        medida(largura)?,
        medida(altura)?,
        medida(passo)?,
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(adicao.map(|a| match a {
        Adicao::Novo(c) => ContatoLido {
            codigo: c.como_texto(),
            novo: true,
        },
        Adicao::JaEstava(c) => ContatoLido {
            codigo: c.como_texto(),
            novo: false,
        },
    }))
}

/// Um contato da agenda desta execução.
pub struct ContatoDaAgenda {
    pub codigo: String,
    /// A chave veio inteira do QR lido pessoalmente.
    pub conferido_por_qr: bool,
}

/// Agenda desta execução, na ordem em que os contatos entraram.
#[flutter_rust_bridge::frb(sync)]
pub fn contatos() -> Vec<ContatoDaAgenda> {
    aparelho::contatos()
        .iter()
        .map(|c| ContatoDaAgenda {
            codigo: c.codigo().como_texto(),
            conferido_por_qr: c.conferencia() == Conferencia::PorQr,
        })
        .collect()
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Desenha a grade como a tela desenha: margem clara, `px` pixels por módulo, e
    /// `sobra` bytes de lixo escuro no fim de cada linha (o `bytesPerRow` da câmera).
    fn foto(g: &GradeDoQr, px: usize, sobra: usize) -> (Vec<u8>, u32, u32) {
        let (lado, margem) = (g.lado as usize, g.margem as usize);
        let largura = (lado + 2 * margem) * px;
        let passo = largura + sobra;
        let mut img = vec![0u8; passo * largura];
        for y in 0..largura {
            for x in 0..largura {
                let (mx, my) = (x / px, y / px);
                let dentro =
                    (margem..margem + lado).contains(&mx) && (margem..margem + lado).contains(&my);
                let escuro = dentro && g.modulos[(my - margem) * lado + (mx - margem)] == 1;
                img[y * passo + x] = if escuro { 0 } else { 255 };
            }
        }
        (img, largura as u32, passo as u32)
    }

    fn codigo_do_erro(r: anyhow::Result<Option<ContatoLido>>) -> String {
        match r {
            Err(e) => e.to_string(),
            Ok(_) => "sem erro".to_string(),
        }
    }

    // Um teste só: o estado do aparelho é global e os testes do cargo rodam em paralelo.
    #[test]
    fn grade_quadro_e_agenda_pela_ponte() -> anyhow::Result<()> {
        esquecer_identidade();
        assert!(grade_propria()?.is_none(), "grade sem identidade");

        // Identidade A mostra o QR.
        let codigo_a = gerar_codigo_contato();
        let grade_a = grade_propria()?.ok_or_else(|| anyhow::anyhow!("sem grade"))?;
        assert_eq!((grade_a.lado, grade_a.margem), (37, 4));
        assert_eq!(grade_a.modulos.len(), 37 * 37);
        assert!(grade_a.modulos.iter().all(|&m| m <= 1));
        let (foto_a, lado, passo) = foto(&grade_a, 4, 64);

        // Identidade B lê o QR de A.
        esquecer_identidade();
        let codigo_b = gerar_codigo_contato();
        assert_ne!(codigo_a, codigo_b);
        let lido = adicionar_contato_do_quadro(foto_a.clone(), lado, lado, passo)?
            .ok_or_else(|| anyhow::anyhow!("não leu"))?;
        assert_eq!((lido.codigo.as_str(), lido.novo), (codigo_a.as_str(), true));
        let de_novo = adicionar_contato_do_quadro(foto_a.clone(), lado, lado, passo)?
            .ok_or_else(|| anyhow::anyhow!("não leu de novo"))?;
        assert!(!de_novo.novo, "o mesmo QR não pode entrar duas vezes");
        let agenda = contatos();
        assert_eq!(agenda.len(), 1);
        assert_eq!(agenda[0].codigo, codigo_a);
        assert!(agenda[0].conferido_por_qr);

        // Sem QR: nada. Passo ignorado (= largura): imagem torta, não lê.
        assert!(adicionar_contato_do_quadro(vec![200; 300 * 300], 300, 300, 300)?.is_none());
        assert!(adicionar_contato_do_quadro(foto_a.clone(), lado, lado, lado)?.is_none());

        // O próprio QR é recusado com o código fixo.
        let (foto_b, lado_b, passo_b) = foto(
            &grade_propria()?.ok_or_else(|| anyhow::anyhow!("sem grade"))?,
            4,
            0,
        );
        assert_eq!(
            codigo_do_erro(adicionar_contato_do_quadro(foto_b, lado_b, lado_b, passo_b)),
            "QR_PROPRIO"
        );
        // Quadro menor do que dizem largura, altura e passo.
        assert_eq!(
            codigo_do_erro(adicionar_contato_do_quadro(vec![0; 10], lado, lado, passo)),
            "QUADRO_INVALIDO"
        );
        assert_eq!(contatos().len(), 1, "recusa não pode mexer na agenda");

        // "Fechar o app" esvazia a agenda.
        esquecer_identidade();
        assert!(contatos().is_empty());
        Ok(())
    }
}
