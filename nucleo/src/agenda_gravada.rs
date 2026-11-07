//! Agenda gravada: a lista de contatos no disco, cifrada com a chave do cofre.
//!
//! Decisão de 07/11/2026: a agenda passa a sobreviver ao fechamento do app. Conversas e
//! mensagens continuam fora do disco; aqui só entram as chaves públicas de cada contato
//! e a forma como foram conferidas. Nada de data, de ordem por conversa recente ou de
//! contagem de mensagens.
//!
//! Formato v1 (documentacao/arquitetura/agenda-gravada.md):
//!
//! ```text
//! arquivo   = cabeçalho || XChaCha20-Poly1305(chave, nonce, claro, aad = cabeçalho)
//! cabeçalho = "KRYAGEND" || versão (1) || nonce (24 bytes, novo a cada gravação)
//! chave     = HKDF-SHA256(sal vazio, ikm = chave do cofre, info = "KRYPTOS/agenda/v1")
//! claro     = quantidade (u16 LE) || MAXIMO_CONTATOS lugares de 98 bytes
//! lugar     = conferência (1 = por QR) || ed25519 (32) || curve25519 (32)
//!             || tamanho do apelido (1) || apelido (32)
//! ```
//!
//! Todos os lugares existem sempre, os vazios em zero: o arquivo tem o mesmo tamanho
//! com 0 ou com 5.000 contatos e não diz quantos você tem. O apelido está reservado e,
//! por enquanto, sempre vazio: a etapa do apelido define esses bytes sem trocar a versão,
//! porque vazio continua valendo.
//!
//! A leitura é estrita. Tamanho diferente, lugar vazio com algum byte fora de zero,
//! conferência desconhecida, chave Ed25519 fora da curva, contato repetido ou em
//! conflito, o próprio cartão: tudo isso é `AgendaCorrompida`. Cada contato do arquivo
//! passa pelas mesmas regras de quando entrou pelo QR.
//!
//! A chave da agenda sai da chave do cofre com outro `info`, então nunca é a mesma da
//! conta. O nonce de 24 bytes é sorteado: com nonce desse tamanho, sortear não tem risco
//! prático de repetir.

use chacha20poly1305::aead::AeadInOut;
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use sha2::Sha256;
use vodozemac::olm::IdentityKeys;
use vodozemac::{Curve25519PublicKey, Ed25519PublicKey};
use zeroize::Zeroizing;

use crate::cartao::CartaoContato;
use crate::cofre::{self, ErroCofre, Resultado, TAMANHO_CHAVE};
use crate::contatos::{Adicao, Agenda, Conferencia, MAXIMO_CONTATOS};

const MAGICO: &[u8; 8] = b"KRYAGEND";
const VERSAO: u8 = 1;
const TAMANHO_NONCE: usize = 24;
const TAMANHO_CABECALHO: usize = MAGICO.len() + 1 + TAMANHO_NONCE;
const TAMANHO_ETIQUETA: usize = 16;
const INFO: &[u8] = b"KRYPTOS/agenda/v1";

const BYTES_CHAVE: usize = 32;
const BYTES_APELIDO: usize = 32;
const BYTES_POR_CONTATO: usize = 1 + BYTES_CHAVE + BYTES_CHAVE + 1 + BYTES_APELIDO;
const BYTES_QUANTIDADE: usize = 2;
const BYTES_CLARO: usize = BYTES_QUANTIDADE + MAXIMO_CONTATOS * BYTES_POR_CONTATO;

/// Tamanho do arquivo, sempre o mesmo: 490.051 bytes (conferido no teste).
pub const TAMANHO_ARQUIVO: usize = TAMANHO_CABECALHO + BYTES_CLARO + TAMANHO_ETIQUETA;

const CONFERIDO_POR_QR: u8 = 1;

// A quantidade vai em 16 bits.
const _: () = assert!(MAXIMO_CONTATOS <= u16::MAX as usize);

fn chave_da_agenda(
    chave_do_cofre: &[u8; TAMANHO_CHAVE],
) -> Resultado<Zeroizing<[u8; TAMANHO_CHAVE]>> {
    let hk = Hkdf::<Sha256>::new(None, chave_do_cofre);
    let mut saida = Zeroizing::new([0u8; TAMANHO_CHAVE]);
    hk.expand(INFO, saida.as_mut())
        .map_err(|_| ErroCofre::ChaveInvalida)?;
    Ok(saida)
}

fn cifra(chave_do_cofre: &[u8; TAMANHO_CHAVE]) -> Resultado<XChaCha20Poly1305> {
    let chave = chave_da_agenda(chave_do_cofre)?;
    Ok(XChaCha20Poly1305::new(&Key::from(*chave)))
}

/// A agenda em claro, no tamanho fixo.
fn claro_de(agenda: &Agenda) -> Resultado<Zeroizing<Vec<u8>>> {
    let contatos = agenda.contatos();
    let quantidade = u16::try_from(contatos.len()).map_err(|_| ErroCofre::AgendaCorrompida)?;
    if contatos.len() > MAXIMO_CONTATOS {
        return Err(ErroCofre::AgendaCorrompida);
    }
    let mut claro = Zeroizing::new(vec![0u8; BYTES_CLARO]);
    claro[..BYTES_QUANTIDADE].copy_from_slice(&quantidade.to_le_bytes());
    for (i, contato) in contatos.iter().enumerate() {
        let inicio = BYTES_QUANTIDADE + i * BYTES_POR_CONTATO;
        let lugar = &mut claro[inicio..inicio + BYTES_POR_CONTATO];
        lugar[0] = match contato.conferencia() {
            Conferencia::PorQr => CONFERIDO_POR_QR,
        };
        let chaves = contato.chaves();
        lugar[1..1 + BYTES_CHAVE].copy_from_slice(chaves.ed25519.as_bytes());
        lugar[1 + BYTES_CHAVE..1 + 2 * BYTES_CHAVE].copy_from_slice(chaves.curve25519.as_bytes());
        // Tamanho do apelido e apelido: zero (reservado).
    }
    Ok(claro)
}

fn selar_claro(
    claro: &[u8],
    chave_do_cofre: &[u8; TAMANHO_CHAVE],
    nonce: [u8; TAMANHO_NONCE],
) -> Resultado<Vec<u8>> {
    let mut cabecalho = [0u8; TAMANHO_CABECALHO];
    cabecalho[..MAGICO.len()].copy_from_slice(MAGICO);
    cabecalho[MAGICO.len()] = VERSAO;
    cabecalho[MAGICO.len() + 1..].copy_from_slice(&nonce);
    let mut corpo = Zeroizing::new(Vec::with_capacity(claro.len() + TAMANHO_ETIQUETA));
    corpo.extend_from_slice(claro);
    // Só falharia com mensagem perto de 256 GiB; o claro tem tamanho fixo de 490 KB.
    cifra(chave_do_cofre)?
        .encrypt_in_place(&XNonce::from(nonce), &cabecalho, &mut *corpo)
        .map_err(|_| ErroCofre::AgendaCorrompida)?;
    let mut arquivo = Vec::with_capacity(TAMANHO_CABECALHO + corpo.len());
    arquivo.extend_from_slice(&cabecalho);
    arquivo.extend_from_slice(&corpo);
    Ok(arquivo)
}

fn selar_com_nonce(
    agenda: &Agenda,
    chave_do_cofre: &[u8; TAMANHO_CHAVE],
    nonce: [u8; TAMANHO_NONCE],
) -> Resultado<Vec<u8>> {
    selar_claro(&claro_de(agenda)?, chave_do_cofre, nonce)
}

/// Monta o conteúdo do arquivo da agenda. Nonce novo a cada chamada.
pub fn selar(agenda: &Agenda, chave_do_cofre: &[u8; TAMANHO_CHAVE]) -> Resultado<Vec<u8>> {
    selar_com_nonce(agenda, chave_do_cofre, cofre::aleatorio()?)
}

/// Abre o conteúdo do arquivo da agenda com a chave do cofre e confere cada contato
/// contra as chaves deste aparelho (`proprio`).
pub fn abrir(
    arquivo: &[u8],
    chave_do_cofre: &[u8; TAMANHO_CHAVE],
    proprio: &IdentityKeys,
) -> Resultado<Agenda> {
    if arquivo.len() <= MAGICO.len() || &arquivo[..MAGICO.len()] != MAGICO {
        return Err(ErroCofre::AgendaCorrompida);
    }
    if arquivo[MAGICO.len()] != VERSAO {
        return Err(ErroCofre::AgendaVersaoDesconhecida);
    }
    if arquivo.len() != TAMANHO_ARQUIVO {
        return Err(ErroCofre::AgendaCorrompida);
    }
    let (cabecalho, cifrado) = arquivo.split_at(TAMANHO_CABECALHO);
    let mut nonce = [0u8; TAMANHO_NONCE];
    nonce.copy_from_slice(&cabecalho[MAGICO.len() + 1..]);
    let mut claro = Zeroizing::new(cifrado.to_vec());
    cifra(chave_do_cofre)?
        .decrypt_in_place(&XNonce::from(nonce), cabecalho, &mut *claro)
        .map_err(|_| ErroCofre::AgendaCorrompida)?;
    ler_claro(&claro, proprio)
}

fn ler_claro(claro: &[u8], proprio: &IdentityKeys) -> Resultado<Agenda> {
    if claro.len() != BYTES_CLARO {
        return Err(ErroCofre::AgendaCorrompida);
    }
    let quantidade = usize::from(u16::from_le_bytes([claro[0], claro[1]]));
    if quantidade > MAXIMO_CONTATOS {
        return Err(ErroCofre::AgendaCorrompida);
    }
    let lugares = &claro[BYTES_QUANTIDADE..];
    let mut agenda = Agenda::nova();
    for (i, lugar) in lugares
        .as_chunks::<BYTES_POR_CONTATO>()
        .0
        .iter()
        .enumerate()
    {
        if i >= quantidade {
            if lugar.iter().any(|&b| b != 0) {
                return Err(ErroCofre::AgendaCorrompida);
            }
            continue;
        }
        if lugar[0] != CONFERIDO_POR_QR || lugar[1 + 2 * BYTES_CHAVE..].iter().any(|&b| b != 0) {
            return Err(ErroCofre::AgendaCorrompida);
        }
        let mut ed = [0u8; BYTES_CHAVE];
        ed.copy_from_slice(&lugar[1..1 + BYTES_CHAVE]);
        let ed25519 = Ed25519PublicKey::from_slice(&ed).map_err(|_| ErroCofre::AgendaCorrompida)?;
        let mut curve = [0u8; BYTES_CHAVE];
        curve.copy_from_slice(&lugar[1 + BYTES_CHAVE..1 + 2 * BYTES_CHAVE]);
        let chaves = IdentityKeys {
            ed25519,
            curve25519: Curve25519PublicKey::from_bytes(curve),
        };
        match agenda.adicionar_conferido_por_qr(CartaoContato::de_chaves(&chaves), proprio) {
            Ok(Adicao::Novo(_)) => {}
            Ok(Adicao::JaEstava(_)) | Err(_) => return Err(ErroCofre::AgendaCorrompida),
        }
    }
    Ok(agenda)
}

#[cfg(test)]
mod testes {
    use std::sync::OnceLock;

    use super::*;
    use crate::contatos::ErroAgenda;
    use crate::identidade::Identidade;

    const CHAVE: [u8; TAMANHO_CHAVE] = [7u8; TAMANHO_CHAVE];
    const NONCE: [u8; TAMANHO_NONCE] = [9u8; TAMANHO_NONCE];

    fn cartao_de(id: &Identidade) -> CartaoContato {
        CartaoContato::de_chaves(&id.chaves_publicas())
    }

    fn agenda_com(eu: &IdentityKeys, quantos: usize) -> Agenda {
        let mut agenda = Agenda::nova();
        for c in cartoes_de_teste().iter().take(quantos) {
            assert!(matches!(
                agenda.adicionar_conferido_por_qr(*c, eu),
                Ok(Adicao::Novo(_))
            ));
        }
        agenda
    }

    /// Cartões de 5.001 identidades distintas, gerados uma vez para todos os testes.
    fn cartoes_de_teste() -> &'static [CartaoContato] {
        static CARTOES: OnceLock<Vec<CartaoContato>> = OnceLock::new();
        CARTOES.get_or_init(|| {
            (0..=MAXIMO_CONTATOS)
                .map(|_| cartao_de(&Identidade::nova()))
                .collect()
        })
    }

    fn eu() -> IdentityKeys {
        static EU: OnceLock<IdentityKeys> = OnceLock::new();
        *EU.get_or_init(|| Identidade::nova().chaves_publicas())
    }

    #[test]
    fn ida_e_volta_mantem_contatos_e_ordem() -> Resultado<()> {
        let agenda = agenda_com(&eu(), 3);
        let arquivo = selar(&agenda, &CHAVE)?;
        assert_eq!(abrir(&arquivo, &CHAVE, &eu())?, agenda);
        Ok(())
    }

    #[test]
    fn tamanho_e_o_mesmo_vazia_e_cheia() -> Resultado<()> {
        let vazia = selar(&Agenda::nova(), &CHAVE)?;
        let cheia_agenda = agenda_com(&eu(), MAXIMO_CONTATOS);
        let cheia = selar(&cheia_agenda, &CHAVE)?;
        assert_eq!(vazia.len(), 490_051);
        assert_eq!(cheia.len(), 490_051);
        assert_eq!(TAMANHO_ARQUIVO, 490_051);
        assert_eq!(abrir(&vazia, &CHAVE, &eu())?, Agenda::nova());
        assert_eq!(abrir(&cheia, &CHAVE, &eu())?, cheia_agenda);
        Ok(())
    }

    #[test]
    fn agenda_cheia_recusa_novo_e_reconhece_quem_ja_esta() {
        let mut agenda = agenda_com(&eu(), MAXIMO_CONTATOS);
        let cartoes = cartoes_de_teste();
        assert_eq!(
            agenda.adicionar_conferido_por_qr(cartoes[MAXIMO_CONTATOS], &eu()),
            Err(ErroAgenda::Cheia)
        );
        assert_eq!(
            agenda.adicionar_conferido_por_qr(cartoes[17], &eu()),
            Ok(Adicao::JaEstava(cartoes[17].codigo()))
        );
        assert_eq!(agenda.contatos().len(), MAXIMO_CONTATOS);
    }

    #[test]
    fn nonce_novo_a_cada_gravacao() -> Resultado<()> {
        let agenda = agenda_com(&eu(), 2);
        let a = selar(&agenda, &CHAVE)?;
        let b = selar(&agenda, &CHAVE)?;
        assert_ne!(
            a[MAGICO.len() + 1..TAMANHO_CABECALHO],
            b[MAGICO.len() + 1..TAMANHO_CABECALHO]
        );
        assert_ne!(a, b);
        Ok(())
    }

    #[test]
    fn outra_chave_do_cofre_nao_abre() -> Resultado<()> {
        let arquivo = selar(&agenda_com(&eu(), 1), &CHAVE)?;
        let mut outra = CHAVE;
        outra[31] ^= 1;
        assert_eq!(
            abrir(&arquivo, &outra, &eu()),
            Err(ErroCofre::AgendaCorrompida)
        );
        Ok(())
    }

    #[test]
    fn qualquer_byte_trocado_e_recusado() -> Resultado<()> {
        let arquivo = selar(&agenda_com(&eu(), 2), &CHAVE)?;
        let posicoes = (0..TAMANHO_CABECALHO + 300)
            .chain(TAMANHO_ARQUIVO / 2..TAMANHO_ARQUIVO / 2 + 64)
            .chain(TAMANHO_ARQUIVO - 64..TAMANHO_ARQUIVO);
        for i in posicoes {
            let mut alterado = arquivo.clone();
            alterado[i] ^= 0x01;
            let esperado = if i == MAGICO.len() {
                ErroCofre::AgendaVersaoDesconhecida
            } else {
                ErroCofre::AgendaCorrompida
            };
            assert_eq!(abrir(&alterado, &CHAVE, &eu()), Err(esperado), "byte {i}");
        }
        Ok(())
    }

    #[test]
    fn tamanho_errado_e_recusado() -> Resultado<()> {
        let arquivo = selar(&Agenda::nova(), &CHAVE)?;
        let mut maior = arquivo.clone();
        maior.push(0);
        for ruim in [
            &arquivo[..arquivo.len() - 1],
            &maior[..],
            &arquivo[..TAMANHO_CABECALHO],
            &arquivo[..MAGICO.len()],
            &[][..],
        ] {
            assert_eq!(abrir(ruim, &CHAVE, &eu()), Err(ErroCofre::AgendaCorrompida));
        }
        Ok(())
    }

    /// Claro montado à mão, cifrado com a chave certa: só as regras de leitura recusam.
    fn com_claro(mexer: impl FnOnce(&mut Vec<u8>)) -> Resultado<Agenda> {
        let mut claro = claro_de(&agenda_com(&eu(), 2))?.to_vec();
        mexer(&mut claro);
        abrir(&selar_claro(&claro, &CHAVE, NONCE)?, &CHAVE, &eu())
    }

    fn lugar(i: usize) -> usize {
        BYTES_QUANTIDADE + i * BYTES_POR_CONTATO
    }

    type Mexer = Box<dyn FnOnce(&mut Vec<u8>)>;

    #[test]
    fn claro_fora_das_regras_e_recusado() -> Resultado<()> {
        let casos: Vec<(&str, Mexer)> = vec![
            (
                "quantidade acima do máximo",
                Box::new(|c| {
                    c[..2].copy_from_slice(
                        &u16::try_from(MAXIMO_CONTATOS + 1)
                            .unwrap_or(0)
                            .to_le_bytes(),
                    );
                }),
            ),
            (
                "quantidade maior que os lugares preenchidos",
                Box::new(|c| c[0] = 3),
            ),
            (
                "lugar vazio com byte fora de zero",
                Box::new(|c| c[lugar(2) + 40] = 1),
            ),
            ("conferência zero", Box::new(|c| c[lugar(0)] = 0)),
            ("conferência desconhecida", Box::new(|c| c[lugar(1)] = 2)),
            ("apelido com tamanho", Box::new(|c| c[lugar(0) + 65] = 1)),
            ("apelido com byte", Box::new(|c| c[lugar(0) + 97] = 1)),
            (
                "contato repetido",
                Box::new(|c| {
                    let primeiro = c[lugar(0)..lugar(1)].to_vec();
                    c[lugar(1)..lugar(2)].copy_from_slice(&primeiro);
                }),
            ),
            (
                "mesma ed25519 com outra curve25519",
                Box::new(|c| {
                    let ed = c[lugar(0) + 1..lugar(0) + 33].to_vec();
                    c[lugar(1) + 1..lugar(1) + 33].copy_from_slice(&ed);
                }),
            ),
            (
                "ed25519 fora da curva",
                Box::new(|c| {
                    // y = 2 não está na curva: a descompressão falha.
                    let mut ruim = [0u8; 32];
                    ruim[0] = 2;
                    c[lugar(0) + 1..lugar(0) + 33].copy_from_slice(&ruim);
                }),
            ),
            (
                "o próprio cartão",
                Box::new(move |c| {
                    c[lugar(1) + 1..lugar(1) + 33].copy_from_slice(eu().ed25519.as_bytes());
                    c[lugar(1) + 33..lugar(1) + 65].copy_from_slice(eu().curve25519.as_bytes());
                }),
            ),
            (
                "claro menor",
                Box::new(|c| {
                    c.pop();
                }),
            ),
            ("claro maior", Box::new(|c| c.push(0))),
        ];
        for (nome, mexer) in casos {
            assert_eq!(com_claro(mexer), Err(ErroCofre::AgendaCorrompida), "{nome}");
        }
        // Sem mexer, o mesmo caminho abre: as recusas acima são das regras, não do teste.
        assert_eq!(com_claro(|_| {})?, agenda_com(&eu(), 2));
        Ok(())
    }

    #[test]
    fn arquivo_nao_tem_chave_de_contato_em_claro() -> Resultado<()> {
        let agenda = agenda_com(&eu(), 3);
        let arquivo = selar(&agenda, &CHAVE)?;
        for c in agenda.contatos() {
            for chave in [
                c.chaves().ed25519.as_bytes(),
                c.chaves().curve25519.as_bytes(),
            ] {
                assert!(
                    !arquivo.windows(8).any(|j| j == &chave[..8]),
                    "pedaço de chave em claro no arquivo"
                );
            }
        }
        Ok(())
    }

    /// Vetor conhecido, conferido pelo oráculo independente
    /// `ferramentas/oraculos/agenda.py` (Python `cryptography`, HChaCha20 escrito à mão).
    #[test]
    fn vetor_conhecido() -> Resultado<()> {
        let chaves = |ed: [u8; 32], cv: [u8; 32]| -> Result<IdentityKeys, ErroCofre> {
            Ok(IdentityKeys {
                ed25519: Ed25519PublicKey::from_slice(&ed).map_err(|_| ErroCofre::ChaveInvalida)?,
                curve25519: Curve25519PublicKey::from_bytes(cv),
            })
        };
        let proprio = Identidade::nova().chaves_publicas();
        let mut agenda = Agenda::nova();
        for (ed, cv) in VETOR_CONTATOS {
            let ed = hex32(ed);
            let cv = hex32(cv);
            assert!(matches!(
                agenda.adicionar_conferido_por_qr(
                    CartaoContato::de_chaves(&chaves(ed, cv)?),
                    &proprio
                ),
                Ok(Adicao::Novo(_))
            ));
        }
        let arquivo = selar_com_nonce(&agenda, &CHAVE, NONCE)?;
        let resumo: String = {
            use sha2::Digest;
            Sha256::digest(&arquivo)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect()
        };
        assert_eq!(resumo, VETOR_SHA256_ARQUIVO);
        assert_eq!(abrir(&arquivo, &CHAVE, &proprio)?, agenda);
        Ok(())
    }

    fn hex32(t: &str) -> [u8; 32] {
        let mut b = [0u8; 32];
        for (i, par) in t.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            let s = std::str::from_utf8(par).unwrap_or("zz");
            b[i] = u8::from_str_radix(s, 16).unwrap_or_default();
        }
        b
    }

    /// Ed25519 e Curve25519 de dois contatos fixos (chaves públicas de vetores do RFC 8032
    /// e do RFC 7748).
    const VETOR_CONTATOS: [(&str, &str); 2] = [
        (
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
            "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a",
        ),
        (
            "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
            "de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f",
        ),
    ];

    const VETOR_SHA256_ARQUIVO: &str =
        "dc55e7553d9e5e75c22d37b8549524804807ad0ab4ecc92b565243cfc3bcbdd5";
}
