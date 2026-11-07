//! Caixa selada: as mensagens recebidas e ainda não queimadas, no disco.
//!
//! Decisões de 07/11/2026 (documentacao/arquitetura/caixa-selada.md):
//! - a mensagem é selada na chegada e só abre quando a pessoa manda descriptografar;
//! - 100 gavetas de tamanho fixo, que existem sempre; gaveta vazia é aleatória;
//! - cada gaveta tem uma chave própria no Keystore (o Kotlin faz o HMAC, como no
//!   cofre). Queimar é sobrescrever a gaveta e trocar a chave dela no hardware;
//! - até 65.536 bytes de texto por mensagem.
//!
//! Gaveta (`gaveta-NN.kryptos`, 65.644 bytes, sempre):
//!
//! ```text
//! arquivo  = nonce (24) || XChaCha20-Poly1305(chave, nonce, claro, aad)
//! chave    = HKDF-SHA256(sal vazio, ikm = HMAC do Keystore sobre o pedido,
//!                        info = "KRYPTOS/gaveta/v1")
//! pedido   = "KRYPTOS/gaveta/v1" || número da gaveta (1) || nonce (24)
//! aad      = "KRYPTOS/gaveta/v1" || número da gaveta (1)
//! claro    = ed25519 do remetente (32) || curve25519 do remetente (32)
//!            || tamanho do texto (u32 LE) || texto, completado com zeros até 65.536
//! ```
//!
//! Sem cabeçalho fixo: a gaveta vazia (bytes aleatórios) não se distingue da ocupada.
//!
//! Índice (`caixa.kryptos`, 4.157 bytes, sempre), cifrado com a chave do cofre:
//!
//! ```text
//! arquivo = "KRYCAIXA" || versão (1) || nonce (24) || XChaCha20-Poly1305(..., aad = cabeçalho)
//! chave   = HKDF-SHA256(sal vazio, ikm = chave do cofre, info = "KRYPTOS/caixa/v1")
//! claro   = próximo número (u64 LE) || 100 × (estado (1) || número (u64 LE) || resumo (32))
//! ```
//!
//! O número ordena a chegada (não é data). O resumo é o SHA-256 da carta, para
//! reconhecer a mesma carta entregue de novo pelo relay.

use std::fs;
use std::io;
use std::path::Path;

use chacha20poly1305::aead::AeadInOut;
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use sha2::Sha256;
use vodozemac::olm::IdentityKeys;
use vodozemac::{Curve25519PublicKey, Ed25519PublicKey};
use zeroize::Zeroizing;

use crate::cofre::{self, ErroCofre, Resultado, TAMANHO_CHAVE};

/// Quantas mensagens não queimadas cabem.
pub const CAPACIDADE: usize = 100;
/// Maior texto de uma mensagem, em bytes.
pub const MAXIMO_TEXTO: usize = 65_536;

pub const NOME_INDICE: &str = "caixa.kryptos";
pub const DOMINIO_GAVETA: &[u8] = b"KRYPTOS/gaveta/v1";

const TAMANHO_NONCE: usize = 24;
const TAMANHO_ETIQUETA: usize = 16;
const BYTES_CHAVE: usize = 32;
const BYTES_REMETENTE: usize = 2 * BYTES_CHAVE;
const BYTES_TAMANHO: usize = 4;
const BYTES_CLARO_GAVETA: usize = BYTES_REMETENTE + BYTES_TAMANHO + MAXIMO_TEXTO;

/// Tamanho de cada gaveta: 65.644 bytes (conferido no teste).
pub const TAMANHO_GAVETA: usize = TAMANHO_NONCE + BYTES_CLARO_GAVETA + TAMANHO_ETIQUETA;
/// Tamanho do pedido que o Kotlin passa no HMAC da chave da gaveta.
pub const TAMANHO_PEDIDO: usize = DOMINIO_GAVETA.len() + 1 + TAMANHO_NONCE;
/// O HMAC-SHA256 do Keystore devolve 32 bytes.
pub const TAMANHO_HMAC: usize = 32;

const MAGICO_INDICE: &[u8; 8] = b"KRYCAIXA";
const VERSAO_INDICE: u8 = 1;
const INFO_INDICE: &[u8] = b"KRYPTOS/caixa/v1";
const TAMANHO_CABECALHO_INDICE: usize = MAGICO_INDICE.len() + 1 + TAMANHO_NONCE;
const BYTES_LUGAR: usize = 1 + 8 + 32;
const BYTES_CLARO_INDICE: usize = 8 + CAPACIDADE * BYTES_LUGAR;
/// Tamanho do índice: 4.157 bytes (conferido no teste).
pub const TAMANHO_INDICE: usize = TAMANHO_CABECALHO_INDICE + BYTES_CLARO_INDICE + TAMANHO_ETIQUETA;

const _: () = assert!(CAPACIDADE <= u8::MAX as usize);
const _: () = assert!(MAXIMO_TEXTO <= u32::MAX as usize);

/// Nome do arquivo de uma gaveta.
pub fn nome_gaveta(gaveta: u8) -> String {
    format!("gaveta-{gaveta:02}.kryptos")
}

/// Situação de uma gaveta no índice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estado {
    Livre,
    /// O índice foi gravado antes da gaveta: se o app parou aqui, a gaveta pode ter
    /// uma mensagem que nenhum índice lista. Ao abrir, ela é queimada.
    Reservada,
    NaoLida,
    /// Já foi mostrada. Queima um minuto depois, ou na hora ao sair do app.
    Lida,
    /// Sobrescrita; falta trocar a chave dela no Keystore antes de usar de novo.
    Queimada,
}

impl Estado {
    fn numero(self) -> u8 {
        match self {
            Self::Livre => 0,
            Self::Reservada => 1,
            Self::NaoLida => 2,
            Self::Lida => 3,
            Self::Queimada => 4,
        }
    }

    fn de_numero(n: u8) -> Option<Self> {
        Some(match n {
            0 => Self::Livre,
            1 => Self::Reservada,
            2 => Self::NaoLida,
            3 => Self::Lida,
            4 => Self::Queimada,
            _ => return None,
        })
    }

    /// Estados em que a gaveta guarda número e resumo de uma carta.
    fn tem_carta(self) -> bool {
        matches!(self, Self::Reservada | Self::NaoLida | Self::Lida)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lugar {
    pub estado: Estado,
    pub numero: u64,
    pub resumo: [u8; 32],
}

impl Lugar {
    const VAZIO: Self = Self {
        estado: Estado::Livre,
        numero: 0,
        resumo: [0; 32],
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Indice {
    pub proximo: u64,
    pub lugares: [Lugar; CAPACIDADE],
}

impl Indice {
    pub fn vazio() -> Self {
        Self {
            proximo: 1,
            lugares: [Lugar::VAZIO; CAPACIDADE],
        }
    }

    fn claro(&self) -> Zeroizing<Vec<u8>> {
        let mut c = Zeroizing::new(Vec::with_capacity(BYTES_CLARO_INDICE));
        c.extend_from_slice(&self.proximo.to_le_bytes());
        for l in &self.lugares {
            c.push(l.estado.numero());
            c.extend_from_slice(&l.numero.to_le_bytes());
            c.extend_from_slice(&l.resumo);
        }
        c
    }

    /// Leitura estrita: estado conhecido; gaveta sem carta com número e resumo em zero;
    /// gaveta com carta com número entre 1 e `proximo - 1`, sem repetir.
    fn de_claro(c: &[u8]) -> Resultado<Self> {
        if c.len() != BYTES_CLARO_INDICE {
            return Err(ErroCofre::CaixaCorrompida);
        }
        let u64_de = |b: &[u8]| {
            let mut q = [0u8; 8];
            q.copy_from_slice(b);
            u64::from_le_bytes(q)
        };
        let proximo = u64_de(&c[..8]);
        if proximo == 0 {
            return Err(ErroCofre::CaixaCorrompida);
        }
        let mut indice = Self {
            proximo,
            lugares: [Lugar::VAZIO; CAPACIDADE],
        };
        let mut numeros = Vec::new();
        for (i, b) in c[8..].as_chunks::<BYTES_LUGAR>().0.iter().enumerate() {
            let estado = Estado::de_numero(b[0]).ok_or(ErroCofre::CaixaCorrompida)?;
            let numero = u64_de(&b[1..9]);
            let mut resumo = [0u8; 32];
            resumo.copy_from_slice(&b[9..]);
            if estado.tem_carta() {
                if numero == 0 || numero >= proximo || numeros.contains(&numero) {
                    return Err(ErroCofre::CaixaCorrompida);
                }
                numeros.push(numero);
            } else if numero != 0 || resumo != [0; 32] {
                return Err(ErroCofre::CaixaCorrompida);
            }
            indice.lugares[i] = Lugar {
                estado,
                numero,
                resumo,
            };
        }
        Ok(indice)
    }
}

fn cifra_do_indice(chave_do_cofre: &[u8; TAMANHO_CHAVE]) -> Resultado<XChaCha20Poly1305> {
    let hk = Hkdf::<Sha256>::new(None, chave_do_cofre);
    let mut chave = Zeroizing::new([0u8; TAMANHO_CHAVE]);
    hk.expand(INFO_INDICE, chave.as_mut())
        .map_err(|_| ErroCofre::ChaveInvalida)?;
    Ok(XChaCha20Poly1305::new(&Key::from(*chave)))
}

fn selar_indice_com_nonce(
    indice: &Indice,
    chave_do_cofre: &[u8; TAMANHO_CHAVE],
    nonce: [u8; TAMANHO_NONCE],
) -> Resultado<Vec<u8>> {
    let mut cabecalho = [0u8; TAMANHO_CABECALHO_INDICE];
    cabecalho[..MAGICO_INDICE.len()].copy_from_slice(MAGICO_INDICE);
    cabecalho[MAGICO_INDICE.len()] = VERSAO_INDICE;
    cabecalho[MAGICO_INDICE.len() + 1..].copy_from_slice(&nonce);
    let mut corpo = indice.claro();
    // Só falharia com mensagem perto de 256 GiB.
    cifra_do_indice(chave_do_cofre)?
        .encrypt_in_place(&XNonce::from(nonce), &cabecalho, &mut *corpo)
        .map_err(|_| ErroCofre::CaixaCorrompida)?;
    let mut arquivo = Vec::with_capacity(TAMANHO_INDICE);
    arquivo.extend_from_slice(&cabecalho);
    arquivo.extend_from_slice(&corpo);
    Ok(arquivo)
}

pub fn selar_indice(indice: &Indice, chave_do_cofre: &[u8; TAMANHO_CHAVE]) -> Resultado<Vec<u8>> {
    selar_indice_com_nonce(indice, chave_do_cofre, cofre::aleatorio()?)
}

pub fn abrir_indice(arquivo: &[u8], chave_do_cofre: &[u8; TAMANHO_CHAVE]) -> Resultado<Indice> {
    if arquivo.len() <= MAGICO_INDICE.len() || &arquivo[..MAGICO_INDICE.len()] != MAGICO_INDICE {
        return Err(ErroCofre::CaixaCorrompida);
    }
    if arquivo[MAGICO_INDICE.len()] != VERSAO_INDICE {
        return Err(ErroCofre::CaixaVersaoDesconhecida);
    }
    if arquivo.len() != TAMANHO_INDICE {
        return Err(ErroCofre::CaixaCorrompida);
    }
    let (cabecalho, cifrado) = arquivo.split_at(TAMANHO_CABECALHO_INDICE);
    let mut nonce = [0u8; TAMANHO_NONCE];
    nonce.copy_from_slice(&cabecalho[MAGICO_INDICE.len() + 1..]);
    let mut claro = Zeroizing::new(cifrado.to_vec());
    cifra_do_indice(chave_do_cofre)?
        .decrypt_in_place(&XNonce::from(nonce), cabecalho, &mut *claro)
        .map_err(|_| ErroCofre::CaixaCorrompida)?;
    Indice::de_claro(&claro)
}

// ---------------------------------------------------------------- gaveta

/// O que o Kotlin passa no HMAC da chave da gaveta.
pub fn pedido(gaveta: u8, nonce: &[u8; TAMANHO_NONCE]) -> [u8; TAMANHO_PEDIDO] {
    let mut p = [0u8; TAMANHO_PEDIDO];
    p[..DOMINIO_GAVETA.len()].copy_from_slice(DOMINIO_GAVETA);
    p[DOMINIO_GAVETA.len()] = gaveta;
    p[DOMINIO_GAVETA.len() + 1..].copy_from_slice(nonce);
    p
}

/// Nonce gravado no começo da gaveta, para montar o pedido de leitura.
pub fn nonce_da_gaveta(arquivo: &[u8]) -> Resultado<[u8; TAMANHO_NONCE]> {
    if arquivo.len() != TAMANHO_GAVETA {
        return Err(ErroCofre::CaixaCorrompida);
    }
    let mut n = [0u8; TAMANHO_NONCE];
    n.copy_from_slice(&arquivo[..TAMANHO_NONCE]);
    Ok(n)
}

fn cifra_da_gaveta(hmac: &[u8]) -> Resultado<XChaCha20Poly1305> {
    if hmac.len() != TAMANHO_HMAC {
        return Err(ErroCofre::ChaveInvalida);
    }
    let hk = Hkdf::<Sha256>::new(None, hmac);
    let mut chave = Zeroizing::new([0u8; TAMANHO_CHAVE]);
    hk.expand(DOMINIO_GAVETA, chave.as_mut())
        .map_err(|_| ErroCofre::ChaveInvalida)?;
    Ok(XChaCha20Poly1305::new(&Key::from(*chave)))
}

fn aad(gaveta: u8) -> Vec<u8> {
    let mut a = DOMINIO_GAVETA.to_vec();
    a.push(gaveta);
    a
}

/// Monta o conteúdo da gaveta. `hmac` é a resposta do Keystore ao `pedido(gaveta,
/// nonce)`. Texto acima de `MAXIMO_TEXTO` é recusado com `TextoGrande`.
pub fn selar_gaveta(
    gaveta: u8,
    nonce: [u8; TAMANHO_NONCE],
    hmac: &[u8],
    remetente: &IdentityKeys,
    texto: &[u8],
) -> Resultado<Vec<u8>> {
    if texto.len() > MAXIMO_TEXTO {
        return Err(ErroCofre::TextoGrande);
    }
    let tamanho = u32::try_from(texto.len()).map_err(|_| ErroCofre::TextoGrande)?;
    let mut corpo = Zeroizing::new(vec![0u8; BYTES_CLARO_GAVETA]);
    corpo[..BYTES_CHAVE].copy_from_slice(remetente.ed25519.as_bytes());
    corpo[BYTES_CHAVE..BYTES_REMETENTE].copy_from_slice(remetente.curve25519.as_bytes());
    corpo[BYTES_REMETENTE..BYTES_REMETENTE + BYTES_TAMANHO].copy_from_slice(&tamanho.to_le_bytes());
    let inicio = BYTES_REMETENTE + BYTES_TAMANHO;
    corpo[inicio..inicio + texto.len()].copy_from_slice(texto);
    cifra_da_gaveta(hmac)?
        .encrypt_in_place(&XNonce::from(nonce), &aad(gaveta), &mut *corpo)
        .map_err(|_| ErroCofre::CaixaCorrompida)?;
    let mut arquivo = Vec::with_capacity(TAMANHO_GAVETA);
    arquivo.extend_from_slice(&nonce);
    arquivo.extend_from_slice(&corpo);
    Ok(arquivo)
}

/// Abre a gaveta com a resposta do Keystore. Devolve o remetente e o texto.
/// Chave trocada, gaveta vazia ou adulterada: `GavetaNaoAbre`.
pub fn abrir_gaveta(
    gaveta: u8,
    arquivo: &[u8],
    hmac: &[u8],
) -> Resultado<(IdentityKeys, Zeroizing<Vec<u8>>)> {
    let nonce = nonce_da_gaveta(arquivo)?;
    let mut claro = Zeroizing::new(arquivo[TAMANHO_NONCE..].to_vec());
    cifra_da_gaveta(hmac)?
        .decrypt_in_place(&XNonce::from(nonce), &aad(gaveta), &mut *claro)
        .map_err(|_| ErroCofre::GavetaNaoAbre)?;
    if claro.len() != BYTES_CLARO_GAVETA {
        return Err(ErroCofre::CaixaCorrompida);
    }
    let mut ed = [0u8; BYTES_CHAVE];
    ed.copy_from_slice(&claro[..BYTES_CHAVE]);
    let mut cv = [0u8; BYTES_CHAVE];
    cv.copy_from_slice(&claro[BYTES_CHAVE..BYTES_REMETENTE]);
    let remetente = IdentityKeys {
        ed25519: Ed25519PublicKey::from_slice(&ed).map_err(|_| ErroCofre::CaixaCorrompida)?,
        curve25519: Curve25519PublicKey::from_bytes(cv),
    };
    let mut t = [0u8; BYTES_TAMANHO];
    t.copy_from_slice(&claro[BYTES_REMETENTE..BYTES_REMETENTE + BYTES_TAMANHO]);
    let tamanho = usize::try_from(u32::from_le_bytes(t)).map_err(|_| ErroCofre::CaixaCorrompida)?;
    let inicio = BYTES_REMETENTE + BYTES_TAMANHO;
    if tamanho > MAXIMO_TEXTO || claro[inicio + tamanho..].iter().any(|&b| b != 0) {
        return Err(ErroCofre::CaixaCorrompida);
    }
    Ok((
        remetente,
        Zeroizing::new(claro[inicio..inicio + tamanho].to_vec()),
    ))
}

/// Gaveta vazia: bytes aleatórios do mesmo tamanho.
pub fn gaveta_vazia() -> Resultado<Vec<u8>> {
    let mut v = vec![0u8; TAMANHO_GAVETA];
    getrandom::fill(&mut v).map_err(|_| ErroCofre::Aleatorio)?;
    Ok(v)
}

// ---------------------------------------------------------------- disco

pub fn gravar_indice(pasta: &Path, conteudo: &[u8]) -> Resultado<()> {
    cofre::gravar_atomico(pasta, NOME_INDICE, conteudo)
}

/// `None`: não há caixa (cofre criado antes dela).
pub fn ler_indice(pasta: &Path) -> Resultado<Option<Vec<u8>>> {
    match fs::read(pasta.join(NOME_INDICE)) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn gravar_gaveta(pasta: &Path, gaveta: u8, conteudo: &[u8]) -> Resultado<()> {
    if conteudo.len() != TAMANHO_GAVETA || usize::from(gaveta) >= CAPACIDADE {
        return Err(ErroCofre::CaixaCorrompida);
    }
    cofre::gravar_atomico(pasta, &nome_gaveta(gaveta), conteudo)
}

pub fn ler_gaveta(pasta: &Path, gaveta: u8) -> Resultado<Vec<u8>> {
    if usize::from(gaveta) >= CAPACIDADE {
        return Err(ErroCofre::CaixaCorrompida);
    }
    match fs::read(pasta.join(nome_gaveta(gaveta))) {
        Ok(b) if b.len() == TAMANHO_GAVETA => Ok(b),
        Ok(_) => Err(ErroCofre::CaixaCorrompida),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(ErroCofre::CaixaCorrompida),
        Err(e) => Err(e.into()),
    }
}

/// Grava uma gaveta e confere lendo de volta, byte a byte.
pub fn gravar_gaveta_conferida(pasta: &Path, gaveta: u8, conteudo: &[u8]) -> Resultado<()> {
    gravar_gaveta(pasta, gaveta, conteudo)?;
    if ler_gaveta(pasta, gaveta)? != conteudo {
        return Err(ErroCofre::CaixaCorrompida);
    }
    Ok(())
}

/// Nomes de todos os arquivos da caixa, para apagar.
pub fn nomes() -> Vec<String> {
    let mut n: Vec<String> = (0..CAPACIDADE)
        .filter_map(|i| u8::try_from(i).ok().map(nome_gaveta))
        .collect();
    n.push(NOME_INDICE.to_string());
    n
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::identidade::Identidade;

    const CHAVE: [u8; TAMANHO_CHAVE] = [3u8; TAMANHO_CHAVE];
    const HMAC: [u8; TAMANHO_HMAC] = [5u8; TAMANHO_HMAC];
    const NONCE: [u8; TAMANHO_NONCE] = [8u8; TAMANHO_NONCE];

    fn indice_com_tres() -> Indice {
        let mut i = Indice::vazio();
        for (g, estado) in [
            (0, Estado::NaoLida),
            (5, Estado::Lida),
            (99, Estado::Reservada),
        ] {
            i.lugares[g] = Lugar {
                estado,
                numero: i.proximo,
                resumo: [u8::try_from(g).unwrap_or(0) + 1; 32],
            };
            i.proximo += 1;
        }
        i.lugares[7].estado = Estado::Queimada;
        i
    }

    #[test]
    fn tamanhos_fixos() -> Resultado<()> {
        assert_eq!(TAMANHO_GAVETA, 65_644);
        assert_eq!(TAMANHO_INDICE, 4_157);
        assert_eq!(TAMANHO_PEDIDO, 42);
        assert_eq!(selar_indice(&Indice::vazio(), &CHAVE)?.len(), 4_157);
        assert_eq!(selar_indice(&indice_com_tres(), &CHAVE)?.len(), 4_157);
        let eu = Identidade::nova().chaves_publicas();
        assert_eq!(selar_gaveta(0, NONCE, &HMAC, &eu, b"")?.len(), 65_644);
        assert_eq!(
            selar_gaveta(0, NONCE, &HMAC, &eu, &vec![b'a'; MAXIMO_TEXTO])?.len(),
            65_644
        );
        assert_eq!(gaveta_vazia()?.len(), 65_644);
        Ok(())
    }

    #[test]
    fn indice_ida_e_volta_e_recusas() -> Resultado<()> {
        let i = indice_com_tres();
        let arq = selar_indice(&i, &CHAVE)?;
        assert_eq!(abrir_indice(&arq, &CHAVE)?, i);
        let mut outra = CHAVE;
        outra[0] ^= 1;
        assert_eq!(abrir_indice(&arq, &outra), Err(ErroCofre::CaixaCorrompida));
        for p in 0..TAMANHO_INDICE {
            let mut a = arq.clone();
            a[p] ^= 0x10;
            let esperado = if p == MAGICO_INDICE.len() {
                ErroCofre::CaixaVersaoDesconhecida
            } else {
                ErroCofre::CaixaCorrompida
            };
            assert_eq!(abrir_indice(&a, &CHAVE), Err(esperado), "byte {p}");
        }
        assert_eq!(
            abrir_indice(&arq[..TAMANHO_INDICE - 1], &CHAVE),
            Err(ErroCofre::CaixaCorrompida)
        );
        Ok(())
    }

    #[test]
    fn indice_fora_das_regras_e_recusado() -> Resultado<()> {
        type Mexer = fn(&mut Vec<u8>);
        let casos: [(&str, Mexer); 7] = [
            ("próximo zero", |c| c[..8].fill(0)),
            ("estado desconhecido", |c| c[8] = 9),
            ("livre com número", |c| c[8 + BYTES_LUGAR + 1] = 1),
            ("livre com resumo", |c| c[8 + BYTES_LUGAR + 20] = 1),
            ("com carta e número zero", |c| c[8 + 1..8 + 9].fill(0)),
            ("número no futuro", |c| c[8 + 1] = 200),
            ("número repetido", |c| {
                let n = c[8 + 1..8 + 9].to_vec();
                c[8 + 5 * BYTES_LUGAR + 1..8 + 5 * BYTES_LUGAR + 9].copy_from_slice(&n);
            }),
        ];
        for (nome, mexer) in casos {
            let mut c = indice_com_tres().claro().to_vec();
            mexer(&mut c);
            assert_eq!(
                Indice::de_claro(&c),
                Err(ErroCofre::CaixaCorrompida),
                "{nome}"
            );
        }
        assert_eq!(
            Indice::de_claro(&indice_com_tres().claro())?,
            indice_com_tres()
        );
        Ok(())
    }

    #[test]
    fn gaveta_ida_e_volta_e_recusas() -> Resultado<()> {
        let rem = Identidade::nova().chaves_publicas();
        let texto = "mensagem com acento: ação".as_bytes();
        let arq = selar_gaveta(4, NONCE, &HMAC, &rem, texto)?;
        let (r, t) = abrir_gaveta(4, &arq, &HMAC)?;
        assert_eq!((r, t.as_slice()), (rem, texto));
        // chave trocada no Keystore: não abre
        let mut outro = HMAC;
        outro[31] ^= 1;
        assert_eq!(
            abrir_gaveta(4, &arq, &outro).err(),
            Some(ErroCofre::GavetaNaoAbre)
        );
        // mudada de lugar (outra gaveta, mesmo HMAC): não abre
        assert_eq!(
            abrir_gaveta(5, &arq, &HMAC).err(),
            Some(ErroCofre::GavetaNaoAbre)
        );
        // vazia: não abre
        assert_eq!(
            abrir_gaveta(4, &gaveta_vazia()?, &HMAC).err(),
            Some(ErroCofre::GavetaNaoAbre)
        );
        // byte trocado em amostra de posições
        for p in (0..TAMANHO_GAVETA).step_by(997).chain([TAMANHO_GAVETA - 1]) {
            let mut a = arq.clone();
            a[p] ^= 1;
            assert_eq!(
                abrir_gaveta(4, &a, &HMAC).err(),
                Some(ErroCofre::GavetaNaoAbre),
                "{p}"
            );
        }
        // tamanho errado
        assert_eq!(
            abrir_gaveta(4, &arq[1..], &HMAC).err(),
            Some(ErroCofre::CaixaCorrompida)
        );
        // texto grande demais
        assert_eq!(
            selar_gaveta(4, NONCE, &HMAC, &rem, &vec![0u8; MAXIMO_TEXTO + 1]).err(),
            Some(ErroCofre::TextoGrande)
        );
        // HMAC de tamanho errado
        assert_eq!(
            selar_gaveta(4, NONCE, &HMAC[..31], &rem, texto).err(),
            Some(ErroCofre::ChaveInvalida)
        );
        Ok(())
    }

    #[test]
    fn gaveta_sem_texto_em_claro_nem_remetente() -> Resultado<()> {
        let rem = Identidade::nova().chaves_publicas();
        let texto = b"TEXTO-PROCURADO-NO-DISCO";
        let arq = selar_gaveta(1, NONCE, &HMAC, &rem, texto)?;
        for agulha in [
            &texto[..],
            &rem.ed25519.as_bytes()[..8],
            &rem.curve25519.as_bytes()[..8],
        ] {
            assert!(!arq.windows(agulha.len()).any(|j| j == agulha));
        }
        Ok(())
    }

    #[test]
    fn pedido_tem_dominio_gaveta_e_nonce() {
        let p = pedido(42, &NONCE);
        assert_eq!(&p[..DOMINIO_GAVETA.len()], DOMINIO_GAVETA);
        assert_eq!(p[DOMINIO_GAVETA.len()], 42);
        assert_eq!(&p[DOMINIO_GAVETA.len() + 1..], &NONCE);
    }

    /// Vetores conhecidos, conferidos pelo oráculo `ferramentas/oraculos/caixa.py`.
    #[test]
    fn vetores_conhecidos() -> Resultado<()> {
        use sha2::Digest;
        let hex = |b: &[u8]| -> String {
            Sha256::digest(b)
                .iter()
                .map(|x| format!("{x:02x}"))
                .collect()
        };
        let rem = IdentityKeys {
            ed25519: Ed25519PublicKey::from_slice(&[
                0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64,
                0x07, 0x3a, 0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68,
                0xf7, 0x07, 0x51, 0x1a,
            ])
            .map_err(|_| ErroCofre::ChaveInvalida)?,
            curve25519: Curve25519PublicKey::from_bytes([0x85; 32]),
        };
        let gaveta = selar_gaveta(7, NONCE, &HMAC, &rem, b"Kryptos")?;
        assert_eq!(hex(&gaveta), VETOR_GAVETA);
        let indice = selar_indice_com_nonce(&indice_com_tres(), &CHAVE, NONCE)?;
        assert_eq!(hex(&indice), VETOR_INDICE);
        Ok(())
    }

    const VETOR_GAVETA: &str = "0aa74af231362518b38e1a503245b8c7199f9c65161267f8adee0546f2baa33e";
    const VETOR_INDICE: &str = "9b3403a80ecb722593ac2585c1687edb0d613eb82bd6833a906ef18991721634";
}
