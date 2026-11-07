//! Funções do núcleo expostas ao app. Cada uma só repassa para `kryptos_nucleo`.
//!
//! Erros chegam ao Dart como `AnyhowException` com o código fixo do núcleo
//! (`PIN_INCORRETO`, `COFRE_CORROMPIDO`...): nada de PIN, chave ou código de contato.

use std::path::Path;

use kryptos_nucleo::aparelho;
use kryptos_nucleo::cofre::ErroCofre;
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

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}
