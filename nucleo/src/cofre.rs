//! Cofre da identidade: a conta Olm gravada cifrada no aparelho.
//!
//! Quem abre o cofre é o PIN junto com uma chave do Keystore do Android, que não sai
//! do hardware. O núcleo faz a parte lenta (Argon2id) e a cifra; o HMAC do Keystore
//! acontece no Kotlin, entre as duas etapas:
//!
//! ```text
//! esticado        = Argon2id(PIN, sal, custo)                      (aqui)
//! chave_do_cofre  = HMAC-SHA256 do Keystore sobre `esticado`       (Kotlin)
//! chave_da_conta  = HKDF-SHA256(sal = nonce, ikm = chave_do_cofre,
//!                               info = INFO || cabeçalho)          (aqui)
//! arquivo         = cabeçalho || conta cifrada pelo vodozemac
//! ```
//!
//! O nonce muda a cada gravação: a cifra de conta do vodozemac tira o IV da própria
//! chave, então repetir a chave repetiria o IV. O cabeçalho entra no `info`: mexer em
//! qualquer byte dele muda a chave e a conta não abre.
//!
//! Não existe verificador do PIN gravado. PIN errado gera outra chave e a autenticação
//! da conta recusa; é isso que o app mostra como "PIN incorreto".
//!
//! Formato e o que esta solução não resolve: `documentacao/arquitetura/cofre-da-identidade.md`.
//!
//! A agenda de contatos fica na mesma pasta, em arquivo próprio (`NOME_AGENDA`), cifrada
//! com uma chave tirada da mesma chave do cofre (`agenda_gravada`). Aqui ficam só a
//! gravação e a leitura do arquivo.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use hkdf::Hkdf;
use sha2::Sha256;
use vodozemac::olm::{Account, AccountPickle};
use zeroize::Zeroizing;

use crate::identidade::Identidade;

/// Dígitos do PIN. A tela usa o mesmo número.
pub const TAMANHO_PIN: usize = 6;
pub const TAMANHO_SAL: usize = 16;
pub const TAMANHO_CHAVE: usize = 32;
const TAMANHO_NONCE: usize = 16;

/// Nome do arquivo do cofre e do contador de tentativas, dentro da pasta que o app
/// informa (no Android, `noBackupFilesDir`: fica fora de qualquer backup).
pub const NOME_COFRE: &str = "identidade.kryptos";
pub const NOME_TENTATIVAS: &str = "tentativas";
pub const NOME_AGENDA: &str = "agenda.kryptos";

const MAGICO: &[u8; 8] = b"KRYCOFRE";
const VERSAO: u8 = 1;
const TAMANHO_CABECALHO: usize = 8 + 1 + TAMANHO_SAL + 12 + TAMANHO_NONCE;
const INFO: &[u8] = b"KRYPTOS/cofre/v1/conta";

/// Custo do Argon2id gravado no cabeçalho, para poder subir depois sem perder cofres antigos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Custo {
    pub memoria_kib: u32,
    pub passadas: u32,
    pub paralelismo: u32,
}

/// Mínimo do OWASP para Argon2id (19 MiB, 2 passadas, 1 linha). O tempo no aparelho é
/// medido pelo teste no celular (`integration_test/cofre_test.dart`).
pub const CUSTO_PADRAO: Custo = Custo {
    memoria_kib: 19_456,
    passadas: 2,
    paralelismo: 1,
};

// Limites para um cabeçalho adulterado não pedir 4 GiB de memória ao aparelho.
const MEMORIA_MAXIMA_KIB: u32 = 1_048_576;
const PASSADAS_MAXIMAS: u32 = 10;
const PARALELISMO_MAXIMO: u32 = 4;

/// Falhas seguidas antes de começar a espera entre tentativas.
pub const FALHAS_SEM_ESPERA: u32 = 4;
const ESPERA_INICIAL_S: u64 = 30;
const ESPERA_MAXIMA_S: u64 = 3_600;
/// Contador ilegível conta como este número de falhas: começa a espera em vez de zerar.
const FALHAS_SE_CONTADOR_ILEGIVEL: u32 = FALHAS_SEM_ESPERA + 1;

/// Erros do cofre. As mensagens são fixas: nenhuma leva PIN, chave ou código (regra 8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroCofre {
    PinInvalido,
    /// PIN errado, arquivo alterado ou chave do Keystore diferente: a cifra não distingue.
    PinIncorreto,
    ArquivoCorrompido,
    VersaoDesconhecida,
    CustoForaDoLimite,
    ChaveInvalida,
    SemIdentidade,
    JaExiste,
    NaoExiste,
    Aleatorio,
    Disco(io::ErrorKind),
    Argon2,
    /// A agenda gravada não abre com a chave do cofre que abriu a identidade, ou abre
    /// fora do formato: arquivo alterado ou danificado.
    AgendaCorrompida,
    AgendaVersaoDesconhecida,
    /// O índice da caixa não abre com a chave do cofre, ou abre fora do formato.
    CaixaCorrompida,
    CaixaVersaoDesconhecida,
    /// A gaveta não abre com a chave do Keystore: chave já trocada (queimada), gaveta
    /// vazia ou alterada.
    GavetaNaoAbre,
    /// Texto acima de `caixa::MAXIMO_TEXTO`.
    TextoGrande,
    /// A busca das formas não pôde ser montada (memória).
    Varredura,
}

impl ErroCofre {
    /// Código estável que atravessa a ponte e que a tela usa para decidir o que mostrar.
    pub fn codigo(&self) -> &'static str {
        match self {
            Self::PinInvalido => "PIN_INVALIDO",
            Self::PinIncorreto => "PIN_INCORRETO",
            Self::ArquivoCorrompido => "COFRE_CORROMPIDO",
            Self::VersaoDesconhecida => "COFRE_VERSAO_DESCONHECIDA",
            Self::CustoForaDoLimite => "COFRE_CUSTO_FORA_DO_LIMITE",
            Self::ChaveInvalida => "CHAVE_INVALIDA",
            Self::SemIdentidade => "SEM_IDENTIDADE",
            Self::JaExiste => "COFRE_JA_EXISTE",
            Self::NaoExiste => "COFRE_NAO_EXISTE",
            Self::Aleatorio => "SEM_ALEATORIO",
            Self::Disco(_) => "DISCO",
            Self::Argon2 => "ARGON2",
            Self::AgendaCorrompida => "AGENDA_CORROMPIDA",
            Self::AgendaVersaoDesconhecida => "AGENDA_VERSAO_DESCONHECIDA",
            Self::CaixaCorrompida => "CAIXA_CORROMPIDA",
            Self::CaixaVersaoDesconhecida => "CAIXA_VERSAO_DESCONHECIDA",
            Self::GavetaNaoAbre => "GAVETA_NAO_ABRE",
            Self::TextoGrande => "TEXTO_GRANDE",
            Self::Varredura => "VARREDURA",
        }
    }
}

impl fmt::Display for ErroCofre {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disco(tipo) => write!(f, "{} ({tipo:?})", self.codigo()),
            _ => f.write_str(self.codigo()),
        }
    }
}

impl std::error::Error for ErroCofre {}

impl From<io::Error> for ErroCofre {
    fn from(e: io::Error) -> Self {
        Self::Disco(e.kind())
    }
}

pub type Resultado<T> = Result<T, ErroCofre>;

/// PIN com exatamente `TAMANHO_PIN` dígitos ASCII.
pub fn validar_pin(pin: &[u8]) -> Resultado<()> {
    if pin.len() == TAMANHO_PIN && pin.iter().all(u8::is_ascii_digit) {
        Ok(())
    } else {
        Err(ErroCofre::PinInvalido)
    }
}

fn conferir_custo(custo: Custo) -> Resultado<()> {
    let p = custo.paralelismo;
    let dentro = (1..=PARALELISMO_MAXIMO).contains(&p)
        && (1..=PASSADAS_MAXIMAS).contains(&custo.passadas)
        && custo.memoria_kib >= 8 * p
        && custo.memoria_kib <= MEMORIA_MAXIMA_KIB;
    if dentro {
        Ok(())
    } else {
        Err(ErroCofre::CustoForaDoLimite)
    }
}

/// Primeira etapa: Argon2id do PIN. O resultado vai para o HMAC do Keystore.
pub fn esticar_pin(
    pin: &[u8],
    sal: &[u8; TAMANHO_SAL],
    custo: Custo,
) -> Resultado<Zeroizing<[u8; TAMANHO_CHAVE]>> {
    validar_pin(pin)?;
    conferir_custo(custo)?;
    let params = Params::new(
        custo.memoria_kib,
        custo.passadas,
        custo.paralelismo,
        Some(TAMANHO_CHAVE),
    )
    .map_err(|_| ErroCofre::CustoForaDoLimite)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut saida = Zeroizing::new([0u8; TAMANHO_CHAVE]);
    argon
        .hash_password_into(pin, sal, saida.as_mut())
        .map_err(|_| ErroCofre::Argon2)?;
    Ok(saida)
}

pub(crate) fn aleatorio<const N: usize>() -> Resultado<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|_| ErroCofre::Aleatorio)?;
    Ok(b)
}

pub fn sal_novo() -> Resultado<[u8; TAMANHO_SAL]> {
    aleatorio()
}

/// Converte a chave que voltou do Keystore (pela ponte) no tamanho certo.
pub fn chave_de(bytes: &[u8]) -> Resultado<Zeroizing<[u8; TAMANHO_CHAVE]>> {
    let mut chave = Zeroizing::new([0u8; TAMANHO_CHAVE]);
    if bytes.len() != TAMANHO_CHAVE {
        return Err(ErroCofre::ChaveInvalida);
    }
    chave.copy_from_slice(bytes);
    Ok(chave)
}

/// Parte do arquivo que não é segredo: o que é preciso para refazer a chave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cabecalho {
    pub sal: [u8; TAMANHO_SAL],
    pub custo: Custo,
    nonce: [u8; TAMANHO_NONCE],
}

impl Cabecalho {
    fn para_bytes(&self) -> [u8; TAMANHO_CABECALHO] {
        let mut b = [0u8; TAMANHO_CABECALHO];
        let mut i = 0;
        let mut por = |parte: &[u8]| {
            b[i..i + parte.len()].copy_from_slice(parte);
            i += parte.len();
        };
        por(MAGICO);
        por(&[VERSAO]);
        por(&self.sal);
        por(&self.custo.memoria_kib.to_le_bytes());
        por(&self.custo.passadas.to_le_bytes());
        por(&self.custo.paralelismo.to_le_bytes());
        por(&self.nonce);
        b
    }

    /// Lê o cabeçalho e devolve o resto do arquivo (a conta cifrada).
    pub fn ler(arquivo: &[u8]) -> Resultado<(Self, &[u8])> {
        if arquivo.len() < MAGICO.len() + 1 || &arquivo[..MAGICO.len()] != MAGICO {
            return Err(ErroCofre::ArquivoCorrompido);
        }
        if arquivo[MAGICO.len()] != VERSAO {
            return Err(ErroCofre::VersaoDesconhecida);
        }
        if arquivo.len() <= TAMANHO_CABECALHO {
            return Err(ErroCofre::ArquivoCorrompido);
        }
        let (cab, corpo) = arquivo.split_at(TAMANHO_CABECALHO);
        let mut i = MAGICO.len() + 1;
        let mut pega = |n: usize| {
            let parte = &cab[i..i + n];
            i += n;
            parte
        };
        let mut sal = [0u8; TAMANHO_SAL];
        sal.copy_from_slice(pega(TAMANHO_SAL));
        let u32_de = |parte: &[u8]| {
            let mut q = [0u8; 4];
            q.copy_from_slice(parte);
            u32::from_le_bytes(q)
        };
        let memoria_kib = u32_de(pega(4));
        let passadas = u32_de(pega(4));
        let paralelismo = u32_de(pega(4));
        let mut nonce = [0u8; TAMANHO_NONCE];
        nonce.copy_from_slice(pega(TAMANHO_NONCE));
        let custo = Custo {
            memoria_kib,
            passadas,
            paralelismo,
        };
        conferir_custo(custo)?;
        Ok((Self { sal, custo, nonce }, corpo))
    }
}

fn chave_da_conta(
    chave_do_cofre: &[u8; TAMANHO_CHAVE],
    cabecalho: &Cabecalho,
) -> Resultado<Zeroizing<[u8; TAMANHO_CHAVE]>> {
    let mut info = Vec::with_capacity(INFO.len() + TAMANHO_CABECALHO);
    info.extend_from_slice(INFO);
    info.extend_from_slice(&cabecalho.para_bytes());
    let hk = Hkdf::<Sha256>::new(Some(&cabecalho.nonce), chave_do_cofre);
    let mut saida = Zeroizing::new([0u8; TAMANHO_CHAVE]);
    hk.expand(&info, saida.as_mut())
        .map_err(|_| ErroCofre::ChaveInvalida)?;
    Ok(saida)
}

fn selar_com_nonce(
    identidade: &Identidade,
    chave_do_cofre: &[u8; TAMANHO_CHAVE],
    sal: [u8; TAMANHO_SAL],
    custo: Custo,
    nonce: [u8; TAMANHO_NONCE],
) -> Resultado<Vec<u8>> {
    conferir_custo(custo)?;
    let cabecalho = Cabecalho { sal, custo, nonce };
    let chave = chave_da_conta(chave_do_cofre, &cabecalho)?;
    let cifrada = identidade.conta().pickle().encrypt(&chave);
    let mut arquivo = Vec::with_capacity(TAMANHO_CABECALHO + cifrada.len());
    arquivo.extend_from_slice(&cabecalho.para_bytes());
    arquivo.extend_from_slice(cifrada.as_bytes());
    Ok(arquivo)
}

/// Monta o conteúdo do arquivo do cofre. Nonce novo a cada chamada.
pub fn selar(
    identidade: &Identidade,
    chave_do_cofre: &[u8; TAMANHO_CHAVE],
    sal: [u8; TAMANHO_SAL],
    custo: Custo,
) -> Resultado<Vec<u8>> {
    selar_com_nonce(identidade, chave_do_cofre, sal, custo, aleatorio()?)
}

/// Abre o conteúdo do arquivo do cofre com a chave que voltou do Keystore.
pub fn abrir(arquivo: &[u8], chave_do_cofre: &[u8; TAMANHO_CHAVE]) -> Resultado<Identidade> {
    let (cabecalho, corpo) = Cabecalho::ler(arquivo)?;
    let texto = std::str::from_utf8(corpo).map_err(|_| ErroCofre::ArquivoCorrompido)?;
    let chave = chave_da_conta(chave_do_cofre, &cabecalho)?;
    let pickle =
        AccountPickle::from_encrypted(texto, &chave).map_err(|_| ErroCofre::PinIncorreto)?;
    Ok(Identidade::de_conta(Account::from_pickle(pickle)))
}

/// Espera antes da próxima tentativa, conforme as falhas seguidas já registradas.
/// Até `FALHAS_SEM_ESPERA`: nenhuma. Depois: 30 s, dobrando, até 1 hora.
pub fn espera(falhas: u32) -> Duration {
    if falhas <= FALHAS_SEM_ESPERA {
        return Duration::ZERO;
    }
    let expoente = (falhas - FALHAS_SEM_ESPERA - 1).min(16);
    let s = ESPERA_INICIAL_S.saturating_mul(1u64 << expoente);
    Duration::from_secs(s.min(ESPERA_MAXIMA_S))
}

// ---------------------------------------------------------------- disco

pub fn caminho_cofre(pasta: &Path) -> PathBuf {
    pasta.join(NOME_COFRE)
}

pub fn existe(pasta: &Path) -> bool {
    caminho_cofre(pasta).is_file()
}

/// Grava em arquivo temporário e troca de uma vez: app fechado no meio não deixa o
/// cofre pela metade. O temporário só tem o que vai no arquivo final (já cifrado).
pub(crate) fn gravar_atomico(pasta: &Path, nome: &str, conteudo: &[u8]) -> Resultado<()> {
    fs::create_dir_all(pasta)?;
    let final_ = pasta.join(nome);
    let temp = pasta.join(format!("{nome}.novo"));
    let resultado = (|| -> io::Result<()> {
        let mut f = fs::File::create(&temp)?;
        f.write_all(conteudo)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&temp, &final_)?;
        sincronizar_pasta(pasta)
    })();
    if let Err(e) = resultado {
        if let Err(e2) = fs::remove_file(&temp)
            && e2.kind() != io::ErrorKind::NotFound
        {
            eprintln!(
                "Kryptos: temporário do cofre ficou no disco ({:?})",
                e2.kind()
            );
        }
        return Err(e.into());
    }
    Ok(())
}

/// Garante que a troca de nome chegou ao disco: sem isto, queda de energia logo
/// depois pode voltar ao estado anterior. Só no Unix (o Android incluído). No Windows
/// a biblioteca padrão não abre pasta como arquivo (dá acesso negado) e o NTFS já
/// registra a troca de nome no próprio journal.
#[cfg(unix)]
fn sincronizar_pasta(pasta: &Path) -> io::Result<()> {
    fs::File::open(pasta)?.sync_all()
}

#[cfg(not(unix))]
fn sincronizar_pasta(_pasta: &Path) -> io::Result<()> {
    Ok(())
}

/// Grava o cofre novo. Recusa se já existe um: trocar a identidade é decisão à parte.
pub fn gravar_novo(pasta: &Path, conteudo: &[u8]) -> Resultado<()> {
    if existe(pasta) {
        return Err(ErroCofre::JaExiste);
    }
    gravar_atomico(pasta, NOME_COFRE, conteudo)?;
    gravar_atomico(pasta, NOME_TENTATIVAS, b"0")
}

/// Troca o conteúdo do cofre que já existe (a conta mudou: chave avulsa gerada ou
/// gasta). Mesma gravação atômica do cofre novo. Recusa se não há cofre: criar é
/// `gravar_novo`, que confere que não existe.
pub fn regravar(pasta: &Path, conteudo: &[u8]) -> Resultado<()> {
    if !existe(pasta) {
        return Err(ErroCofre::NaoExiste);
    }
    gravar_atomico(pasta, NOME_COFRE, conteudo)
}

pub fn ler(pasta: &Path) -> Resultado<Vec<u8>> {
    match fs::read(caminho_cofre(pasta)) {
        Ok(b) => Ok(b),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(ErroCofre::NaoExiste),
        Err(e) => Err(e.into()),
    }
}

/// Grava a agenda (já cifrada) com a mesma troca atômica do cofre.
pub fn gravar_agenda(pasta: &Path, conteudo: &[u8]) -> Resultado<()> {
    gravar_atomico(pasta, NOME_AGENDA, conteudo)
}

/// Conteúdo da agenda gravada. `None`: não há arquivo (cofre criado antes da agenda
/// gravada existir).
pub fn ler_agenda(pasta: &Path) -> Resultado<Option<Vec<u8>>> {
    match fs::read(pasta.join(NOME_AGENDA)) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Falhas seguidas registradas. Arquivo ausente: zero. Ilegível: conta como
/// `FALHAS_SE_CONTADOR_ILEGIVEL`, para apagar o contador não zerar a espera.
pub fn ler_tentativas(pasta: &Path) -> Resultado<u32> {
    match fs::read_to_string(pasta.join(NOME_TENTATIVAS)) {
        Ok(t) => Ok(t
            .trim()
            .parse::<u32>()
            .unwrap_or(FALHAS_SE_CONTADOR_ILEGIVEL)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // Cofre sem contador: alguém apagou. Mesma regra do ilegível.
            if existe(pasta) {
                Ok(FALHAS_SE_CONTADOR_ILEGIVEL)
            } else {
                Ok(0)
            }
        }
        Err(e) if e.kind() == io::ErrorKind::InvalidData => Ok(FALHAS_SE_CONTADOR_ILEGIVEL),
        Err(e) => Err(e.into()),
    }
}

pub fn gravar_tentativas(pasta: &Path, falhas: u32) -> Resultado<()> {
    gravar_atomico(pasta, NOME_TENTATIVAS, falhas.to_string().as_bytes())
}

/// Apaga caixa, agenda, cofre e contador. Usado pelo teste no aparelho e, adiante,
/// pelo "apagar tudo". Sem as chaves do Keystore, que o Kotlin apaga: são elas que
/// deixam ilegível o que sobrar no armazenamento depois de apagado.
pub fn apagar(pasta: &Path) -> Resultado<()> {
    let mut nomes = crate::caixa::nomes();
    nomes.extend([NOME_AGENDA, NOME_COFRE, NOME_TENTATIVAS].map(String::from));
    for nome in nomes {
        match fs::remove_file(pasta.join(nome)) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- varredura

/// Resultado da busca por segredos da identidade em uma pasta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Varredura {
    pub arquivos: u32,
    pub ocorrencias: u32,
    pub ilegiveis: u32,
}

/// Formas em que um segredo poderia ter sido gravado por engano.
fn formas(segredo: &[u8]) -> Vec<Zeroizing<Vec<u8>>> {
    let mut f: Vec<Zeroizing<Vec<u8>>> = vec![Zeroizing::new(segredo.to_vec())];
    let hex: String = segredo.iter().map(|b| format!("{b:02x}")).collect();
    f.push(Zeroizing::new(hex.to_uppercase().into_bytes()));
    f.push(Zeroizing::new(hex.into_bytes()));
    for motor in [STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD] {
        f.push(Zeroizing::new(motor.encode(segredo).into_bytes()));
    }
    // Como o serde_json escreve uma lista de bytes: [3,118,...].
    let lista: Vec<String> = segredo.iter().map(u8::to_string).collect();
    f.push(Zeroizing::new(
        format!("[{}]", lista.join(",")).into_bytes(),
    ));
    f
}

/// Procura os segredos dados, em todas as formas de `formas`, em todos os arquivos da
/// pasta (recursivo, sem seguir atalhos). Devolve só contagens: `ocorrencias` soma,
/// em cada arquivo, quantas formas aparecem nele.
///
/// Uma passada só por arquivo para todas as formas (Aho-Corasick, com sobreposição):
/// com a caixa, a pasta passou a ter 6,5 MB, e comparar forma por forma ficava lento
/// demais no celular com o núcleo sem otimização (o teste no aparelho).
pub fn procurar(pasta: &Path, segredos: &[Zeroizing<Vec<u8>>]) -> Resultado<Varredura> {
    let agulhas: Vec<Zeroizing<Vec<u8>>> = segredos
        .iter()
        .flat_map(|s| formas(s))
        .filter(|a| !a.is_empty())
        .collect();
    let busca = aho_corasick::AhoCorasick::new(agulhas.iter().map(|a| a.as_slice()))
        .map_err(|_| ErroCofre::Varredura)?;
    let mut v = Varredura::default();
    let mut pendentes = vec![pasta.to_path_buf()];
    while let Some(dir) = pendentes.pop() {
        let entradas = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => {
                v.ilegiveis += 1;
                continue;
            }
        };
        for entrada in entradas {
            let Ok(entrada) = entrada else {
                v.ilegiveis += 1;
                continue;
            };
            let Ok(tipo) = entrada.file_type() else {
                v.ilegiveis += 1;
                continue;
            };
            if tipo.is_dir() {
                pendentes.push(entrada.path());
            } else if tipo.is_file() {
                match fs::read(entrada.path()) {
                    Ok(conteudo) => {
                        let conteudo = Zeroizing::new(conteudo);
                        v.arquivos += 1;
                        let mut achadas = vec![false; agulhas.len()];
                        for m in busca.find_overlapping_iter(conteudo.as_slice()) {
                            achadas[m.pattern().as_usize()] = true;
                        }
                        let n = achadas.iter().filter(|&&a| a).count();
                        v.ocorrencias = v
                            .ocorrencias
                            .saturating_add(u32::try_from(n).unwrap_or(u32::MAX));
                    }
                    Err(_) => v.ilegiveis += 1,
                }
            }
        }
    }
    Ok(v)
}

#[cfg(test)]
mod testes {
    use super::*;

    type R = Result<(), Box<dyn std::error::Error>>;

    /// Custo baixo para os testes rodarem rápido; o padrão tem teste próprio.
    const BARATO: Custo = Custo {
        memoria_kib: 64,
        passadas: 1,
        paralelismo: 1,
    };

    fn pasta_temp(nome: &str) -> Result<PathBuf, io::Error> {
        let p = std::env::temp_dir().join(format!("kryptos-teste-{}-{nome}", std::process::id()));
        if p.exists() {
            fs::remove_dir_all(&p)?;
        }
        fs::create_dir_all(&p)?;
        Ok(p)
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    // ---------- vetores conhecidos (oráculo em Python, separado do Rust) ----------

    #[test]
    fn vetor_conhecido_argon2id() -> R {
        let sal: [u8; 16] = core::array::from_fn(|i| i as u8);
        let saida = esticar_pin(b"123456", &sal, BARATO)?;
        assert_eq!(hex(saida.as_ref()), VETOR_ARGON2_BARATO);
        Ok(())
    }

    #[test]
    fn vetor_conhecido_argon2id_custo_padrao() -> R {
        let sal: [u8; 16] = core::array::from_fn(|i| 0xA0 + i as u8);
        let saida = esticar_pin(b"908172", &sal, CUSTO_PADRAO)?;
        assert_eq!(hex(saida.as_ref()), VETOR_ARGON2_PADRAO);
        Ok(())
    }

    #[test]
    fn vetor_conhecido_chave_da_conta() -> R {
        let cab = Cabecalho {
            sal: core::array::from_fn(|i| i as u8),
            custo: CUSTO_PADRAO,
            nonce: core::array::from_fn(|i| 0x40 + i as u8),
        };
        assert_eq!(hex(&cab.para_bytes()), VETOR_CABECALHO);
        let chave_do_cofre: [u8; 32] = core::array::from_fn(|i| 0x20 + i as u8);
        let chave = chave_da_conta(&chave_do_cofre, &cab)?;
        assert_eq!(hex(chave.as_ref()), VETOR_CHAVE_DA_CONTA);
        Ok(())
    }

    #[test]
    fn vetor_conhecido_arquivo_inteiro() -> R {
        let identidade = Identidade::de_pickle_json(CONTA_FIXA_JSON)?;
        // Mesmas chaves do vetor do código de contato (RFC 8032 e RFC 7748).
        assert_eq!(identidade.codigo_contato().como_texto(), "2J2F-P63Y");
        let chave_do_cofre: [u8; 32] = core::array::from_fn(|i| 0x20 + i as u8);
        let arquivo = selar_com_nonce(
            &identidade,
            &chave_do_cofre,
            core::array::from_fn(|i| i as u8),
            CUSTO_PADRAO,
            core::array::from_fn(|i| 0x40 + i as u8),
        )?;
        assert_eq!(hex(&arquivo), VETOR_ARQUIVO);
        let aberta = abrir(&arquivo, &chave_do_cofre)?;
        assert_eq!(aberta.codigo_contato(), identidade.codigo_contato());
        Ok(())
    }

    // ---------- ida e volta ----------

    #[test]
    fn selar_e_abrir_devolve_a_mesma_identidade() -> R {
        let identidade = Identidade::nova();
        let chave = [7u8; 32];
        let arquivo = selar(&identidade, &chave, sal_novo()?, BARATO)?;
        let aberta = abrir(&arquivo, &chave)?;
        assert_eq!(aberta.codigo_contato(), identidade.codigo_contato());
        assert_eq!(aberta.chaves_publicas(), identidade.chaves_publicas());
        Ok(())
    }

    #[test]
    fn duas_gravacoes_da_mesma_conta_saem_diferentes() -> R {
        // Nonce novo a cada gravação: mesma chave do cofre, chave da conta diferente.
        let identidade = Identidade::nova();
        let chave = [7u8; 32];
        let sal = sal_novo()?;
        let a = selar(&identidade, &chave, sal, BARATO)?;
        let b = selar(&identidade, &chave, sal, BARATO)?;
        assert_ne!(a, b);
        assert_ne!(a[TAMANHO_CABECALHO..], b[TAMANHO_CABECALHO..]);
        Ok(())
    }

    // ---------- caso errado e adulteração ----------

    #[test]
    fn chave_errada_nao_abre() -> R {
        let identidade = Identidade::nova();
        let arquivo = selar(&identidade, &[7u8; 32], sal_novo()?, BARATO)?;
        assert_eq!(
            abrir(&arquivo, &[8u8; 32]).err(),
            Some(ErroCofre::PinIncorreto)
        );
        Ok(())
    }

    #[test]
    fn pin_errado_da_outra_chave() -> R {
        let sal = sal_novo()?;
        let certo = esticar_pin(b"135790", &sal, BARATO)?;
        let errado = esticar_pin(b"135791", &sal, BARATO)?;
        assert_ne!(certo.as_ref(), errado.as_ref());
        Ok(())
    }

    #[test]
    fn qualquer_bit_alterado_e_recusado() -> R {
        let identidade = Identidade::nova();
        let chave = [7u8; 32];
        let arquivo = selar(&identidade, &chave, sal_novo()?, BARATO)?;
        for i in 0..arquivo.len() {
            for bit in [0x01u8, 0x80] {
                let mut alterado = arquivo.clone();
                alterado[i] ^= bit;
                assert!(
                    abrir(&alterado, &chave).is_err(),
                    "byte {i} alterado (bit {bit:#x}) e ainda abriu"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn arquivo_vazio_cortado_ou_estranho_da_erro_sem_panico() -> R {
        let identidade = Identidade::nova();
        let chave = [7u8; 32];
        let arquivo = selar(&identidade, &chave, sal_novo()?, BARATO)?;
        for n in 0..arquivo.len() {
            assert!(
                abrir(&arquivo[..n], &chave).is_err(),
                "cortado em {n} abriu"
            );
        }
        assert_eq!(
            abrir(b"qualquer coisa", &chave).err(),
            Some(ErroCofre::ArquivoCorrompido)
        );
        let mut outra_versao = arquivo.clone();
        outra_versao[8] = 2;
        assert_eq!(
            abrir(&outra_versao, &chave).err(),
            Some(ErroCofre::VersaoDesconhecida)
        );
        Ok(())
    }

    #[test]
    fn custo_adulterado_alem_do_limite_e_recusado_antes_do_argon2() -> R {
        let identidade = Identidade::nova();
        let chave = [7u8; 32];
        let mut arquivo = selar(&identidade, &chave, sal_novo()?, BARATO)?;
        // memória = 4 GiB - 1 KiB
        arquivo[25..29].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            Cabecalho::ler(&arquivo).err(),
            Some(ErroCofre::CustoForaDoLimite)
        );
        Ok(())
    }

    #[test]
    fn pin_fora_do_formato_e_recusado() {
        let sal = [0u8; 16];
        for pin in [
            &b""[..],
            b"12345",
            b"1234567",
            b"12345a",
            b"12 456",
            "12345\u{663}".as_bytes(),
        ] {
            assert_eq!(
                esticar_pin(pin, &sal, BARATO).err(),
                Some(ErroCofre::PinInvalido),
                "aceitou {pin:?}"
            );
        }
    }

    #[test]
    fn chave_do_keystore_com_tamanho_errado_e_recusada() {
        for n in [0usize, 16, 31, 33, 64] {
            assert_eq!(
                chave_de(&vec![1u8; n]).err(),
                Some(ErroCofre::ChaveInvalida)
            );
        }
        assert!(chave_de(&[1u8; 32]).is_ok());
    }

    #[test]
    fn mensagens_de_erro_nao_levam_nada_variavel() {
        let todos = [
            ErroCofre::PinInvalido,
            ErroCofre::PinIncorreto,
            ErroCofre::ArquivoCorrompido,
            ErroCofre::VersaoDesconhecida,
            ErroCofre::CustoForaDoLimite,
            ErroCofre::ChaveInvalida,
            ErroCofre::SemIdentidade,
            ErroCofre::JaExiste,
            ErroCofre::NaoExiste,
            ErroCofre::Aleatorio,
            ErroCofre::Argon2,
        ];
        for e in todos {
            assert_eq!(e.to_string(), e.codigo());
            assert!(
                e.codigo()
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
            );
        }
    }

    // ---------- espera entre tentativas ----------

    #[test]
    fn espera_cresce_e_para_no_teto() {
        let s = |f| espera(f).as_secs();
        assert_eq!((s(0), s(1), s(4)), (0, 0, 0));
        assert_eq!((s(5), s(6), s(7), s(8)), (30, 60, 120, 240));
        assert_eq!(s(12), 3_600);
        assert_eq!(s(u32::MAX), 3_600);
    }

    // ---------- disco ----------

    #[test]
    fn gravar_ler_e_recusar_segundo_cofre() -> R {
        let pasta = pasta_temp("gravar")?;
        let r = (|| -> R {
            assert!(!existe(&pasta));
            assert_eq!(ler(&pasta).err(), Some(ErroCofre::NaoExiste));
            gravar_novo(&pasta, b"conteudo-1")?;
            assert!(existe(&pasta));
            assert_eq!(ler(&pasta)?, b"conteudo-1");
            assert_eq!(
                gravar_novo(&pasta, b"conteudo-2").err(),
                Some(ErroCofre::JaExiste)
            );
            assert_eq!(
                ler(&pasta)?,
                b"conteudo-1",
                "segundo cofre sobrescreveu o primeiro"
            );
            assert!(!pasta.join(format!("{NOME_COFRE}.novo")).exists());
            assert_eq!(ler_tentativas(&pasta)?, 0);
            regravar(&pasta, b"conteudo-3")?;
            assert_eq!(ler(&pasta)?, b"conteudo-3");
            assert!(!pasta.join(format!("{NOME_COFRE}.novo")).exists());
            apagar(&pasta)?;
            assert!(!existe(&pasta));
            assert_eq!(
                regravar(&pasta, b"conteudo-4").err(),
                Some(ErroCofre::NaoExiste),
                "regravar criou cofre"
            );
            assert!(!existe(&pasta));
            apagar(&pasta)?; // de novo, sem nada: não é erro
            Ok(())
        })();
        fs::remove_dir_all(&pasta)?;
        r
    }

    #[test]
    fn contador_ausente_ou_ilegivel_nao_zera_a_espera() -> R {
        let pasta = pasta_temp("contador")?;
        let r = (|| -> R {
            assert_eq!(ler_tentativas(&pasta)?, 0, "sem cofre e sem contador: zero");
            gravar_novo(&pasta, b"x")?;
            gravar_tentativas(&pasta, 3)?;
            assert_eq!(ler_tentativas(&pasta)?, 3);
            fs::write(pasta.join(NOME_TENTATIVAS), b"lixo")?;
            assert_eq!(ler_tentativas(&pasta)?, FALHAS_SE_CONTADOR_ILEGIVEL);
            fs::remove_file(pasta.join(NOME_TENTATIVAS))?;
            assert_eq!(ler_tentativas(&pasta)?, FALHAS_SE_CONTADOR_ILEGIVEL);
            assert!(espera(FALHAS_SE_CONTADOR_ILEGIVEL) > Duration::ZERO);
            Ok(())
        })();
        fs::remove_dir_all(&pasta)?;
        r
    }

    // ---------- varredura: calibrada com casos certos e errados (regra 5.3) ----------

    #[test]
    fn varredura_acha_o_segredo_em_cada_forma_plantada() -> R {
        let pasta = pasta_temp("varredura-certa")?;
        let r = (|| -> R {
            let segredo: Vec<u8> = (0u8..32)
                .map(|i| i.wrapping_mul(37).wrapping_add(11))
                .collect();
            let todas = formas(&segredo);
            assert_eq!(todas.len(), 8);
            fs::create_dir_all(pasta.join("sub/funda"))?;
            for (i, forma) in todas.iter().enumerate() {
                let mut conteudo = b"antes ".to_vec();
                conteudo.extend_from_slice(forma);
                conteudo.extend_from_slice(b" depois");
                fs::write(pasta.join("sub/funda").join(format!("f{i}")), conteudo)?;
            }
            let v = procurar(&pasta, &[Zeroizing::new(segredo)])?;
            assert_eq!(v.arquivos, 8);
            assert!(v.ocorrencias >= 8, "achou só {} de 8", v.ocorrencias);
            assert_eq!(v.ilegiveis, 0);
            Ok(())
        })();
        fs::remove_dir_all(&pasta)?;
        r
    }

    #[test]
    fn varredura_nao_acha_nada_no_cofre_gravado() -> R {
        let pasta = pasta_temp("varredura-cofre")?;
        let r = (|| -> R {
            let identidade = Identidade::nova();
            let arquivo = selar(&identidade, &[7u8; 32], sal_novo()?, BARATO)?;
            gravar_novo(&pasta, &arquivo)?;
            let segredos = identidade.segredos_para_varredura()?;
            assert_eq!(segredos.len(), 2);
            let v = procurar(&pasta, &segredos)?;
            assert_eq!(v.arquivos, 2, "cofre + contador");
            assert_eq!(v.ocorrencias, 0);
            // Controle: a mesma conta gravada sem cifra é achada.
            let em_claro = identidade.pickle_em_claro_para_teste()?;
            fs::write(pasta.join("vazamento.json"), em_claro.as_slice())?;
            let v = procurar(&pasta, &segredos)?;
            assert!(v.ocorrencias >= 2, "não achou a conta em claro");
            Ok(())
        })();
        fs::remove_dir_all(&pasta)?;
        r
    }

    // Valores tirados de ferramentas/oraculos/cofre.py (Python, sem o código Rust).
    const VETOR_ARGON2_BARATO: &str =
        "8690aca49c94d0beaca4f6669cebe9b5acdf625c8f5c91a0d13fa3a7f4ab7908";
    const VETOR_ARGON2_PADRAO: &str =
        "08a00c7b0ca48c5859ab830ce03c7e51941ff6c5fa362738c1e3095676498cd6";
    const VETOR_CABECALHO: &str = "4b5259434f46524501000102030405060708090a0b0c0d0e0f004c00000200000001000000404142434445464748494a4b4c4d4e4f";
    const VETOR_CHAVE_DA_CONTA: &str =
        "c64e620ff5647e2f7e423deee79600563507389ac64bdf4a4bde9a5c0ce3d517";
    const VETOR_ARQUIVO: &str = "4b5259434f46524501000102030405060708090a0b0c0d0e0f004c00000200000001000000404142434445464748494a4b4c4d4e4f636c753241366c55647169644332755533723855326e776f6150364736385250742b52576e506d464b6a5948435a3268446e64384c4779344c532b565a6f456a617551366f69696c496f66375269626765495777447169782b7677736b4f416c62303979494a494d3873316446366d7349714957724742786e7762752b65504f4c374452324d5544344c507141464937745341774a654d67786246516d50533355447272696234546a3066694d6842346d32484567646637594339394a2b75437a4f6b6e4c6b5232367647644f466c53674e6b62773971725a44692f7545544c354d663649766b514b66774b72757466366e78325330734b647354473871614e636666674a7930505455517144574568457479384172394442354967584c316836416e46596e38336947364b33414f4273465a7938564b3474446c5a367559753961597536464d65464a6d5152706635346e38726a6a4a5270646c552f456c73547844527244776c5a6e4343436661564a6e466d485164457064724f2b3950784966685a7674356d31446f4c45656371705253437077534338306f4b724877357a37392f4476514d48614a65794f4b57536e7a747a73594a5a4e3848306e4a79775651756c616d532b7a5834585346692b56704f3473684d37325a6246324c4b325078436947474331585256794b77384d7049746a7038556b704475582b3734396a69326c336c4d3856795679456e4b417462782f39514d724c7950627263706372613454384b2f505a526b4f52684742474b3645614d305a5149454f2b334f356e446b43497653657077";
    const CONTA_FIXA_JSON: &str = r#"{"signing_key":{"Normal":[157,97,177,157,239,253,90,96,186,132,74,244,146,236,44,196,68,73,197,105,123,50,105,25,112,59,172,3,28,174,127,96]},"diffie_hellman_key":[119,7,109,10,115,24,165,125,60,22,193,114,81,178,102,69,223,76,47,135,235,192,153,42,177,119,251,165,29,185,44,42],"one_time_keys":{"next_key_id":0,"public_keys":{},"private_keys":{}},"fallback_keys":{"key_id":0,"fallback_key":null,"previous_fallback_key":null}}"#;
}
