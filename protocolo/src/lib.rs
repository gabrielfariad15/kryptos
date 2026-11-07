//! Formato das mensagens entre o app e o relay do Kryptos (documentacao/arquitetura/estoque-de-chaves.md).
//!
//! Publicação (o dono põe chaves avulsas no estoque do relay):
//!
//! ```text
//! corpo      = versão (1) || dono (32, Ed25519) || número (8, u64 LE) || n (2, u16 LE)
//!              || n × (chave Curve25519 (32) || assinatura da chave (64))
//! publicação = corpo || assinatura (64) = Ed25519 do dono sobre DOMINIO_PUBLICACAO || corpo
//! ```
//!
//! - O número só sobe: o relay recusa número igual ou menor que o último aceito, e
//!   assim uma publicação copiada não pode ser repetida. Não depende do relógio.
//! - `n` vai de 0 a `MAXIMO_POR_PUBLICACAO`. Com 0, a publicação só consulta o estoque.
//! - A leitura é estrita: tamanho exato, sem byte a mais.
//!
//! Retirada (alguém pega uma chave avulsa do dono): a resposta é uma chave assinada,
//! 96 bytes. Quem retira confere a assinatura da chave contra a Ed25519 lida no QR
//! (`kryptos_nucleo::sessao`); o relay não precisa conferir.
//!
//! Carta (a mensagem cifrada, por qualquer caminho): `carta`.
//!
//! Malote (cartas esperando o destinatário no relay; busca assinada e entrega): `malote`.
//!
//! Regra 8.2: o `Debug` não mostra chave, e os erros não carregam bytes recebidos.

pub mod carta;
pub mod malote;

use std::fmt;

use vodozemac::{Ed25519PublicKey, Ed25519Signature};

pub const VERSAO: u8 = 1;

/// Separa a assinatura da publicação de qualquer outra coisa que a mesma Ed25519
/// assinar. Mudar o formato exige trocar o `v1` e a `VERSAO`.
pub const DOMINIO_PUBLICACAO: &[u8] = b"KRYPTOS/publicacao/v1";

pub const BYTES_CHAVE: usize = 32;
pub const BYTES_ASSINATURA: usize = 64;
/// Uma chave avulsa assinada: chave e assinatura.
pub const BYTES_CHAVE_ASSINADA: usize = BYTES_CHAVE + BYTES_ASSINATURA;

/// Chaves por publicação. Estimo que 100 sobra: o estoque de cada dono é menor.
pub const MAXIMO_POR_PUBLICACAO: usize = 100;

const BYTES_CABECALHO: usize = 1 + BYTES_CHAVE + 8 + 2;
/// Publicação sem chave (só consulta).
pub const BYTES_MINIMO_PUBLICACAO: usize = BYTES_CABECALHO + BYTES_ASSINATURA;
/// Maior publicação aceita. O relay limita o corpo do pedido por este número.
pub const BYTES_MAXIMO_PUBLICACAO: usize =
    BYTES_MINIMO_PUBLICACAO + MAXIMO_POR_PUBLICACAO * BYTES_CHAVE_ASSINADA;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroProtocolo {
    /// Primeiro byte diferente de `VERSAO`.
    Versao,
    /// Tamanho que não fecha com o número de chaves declarado.
    Tamanho,
    /// Mais chaves que `MAXIMO_POR_PUBLICACAO`.
    Demais,
    /// O dono não é uma chave Ed25519 válida.
    ChaveInvalida,
    /// A assinatura não é do dono sobre esta publicação.
    Assinatura,
    /// Dono em texto fora do formato (64 caracteres hexadecimais minúsculos).
    Dono,
    /// Tipo de mensagem desconhecido numa carta.
    Tipo,
    /// O mesmo id duas vezes numa busca ou numa entrega do malote.
    Repetida,
}

impl ErroProtocolo {
    pub fn codigo(&self) -> &'static str {
        match self {
            Self::Versao => "PROTOCOLO_VERSAO",
            Self::Tamanho => "PROTOCOLO_TAMANHO",
            Self::Demais => "PROTOCOLO_DEMAIS",
            Self::ChaveInvalida => "PROTOCOLO_CHAVE_INVALIDA",
            Self::Assinatura => "PROTOCOLO_ASSINATURA",
            Self::Dono => "PROTOCOLO_DONO",
            Self::Tipo => "PROTOCOLO_TIPO",
            Self::Repetida => "PROTOCOLO_REPETIDA",
        }
    }
}

impl fmt::Display for ErroProtocolo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErroProtocolo {}

/// Uma chave avulsa com a assinatura que o dono fez sobre ela.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ChaveAssinada {
    pub chave: [u8; BYTES_CHAVE],
    pub assinatura: [u8; BYTES_ASSINATURA],
}

impl ChaveAssinada {
    pub fn para_bytes(&self) -> [u8; BYTES_CHAVE_ASSINADA] {
        let mut b = [0u8; BYTES_CHAVE_ASSINADA];
        b[..BYTES_CHAVE].copy_from_slice(&self.chave);
        b[BYTES_CHAVE..].copy_from_slice(&self.assinatura);
        b
    }

    /// Lê a resposta de uma retirada: exatamente 96 bytes.
    pub fn de_bytes(bytes: &[u8]) -> Result<Self, ErroProtocolo> {
        if bytes.len() != BYTES_CHAVE_ASSINADA {
            return Err(ErroProtocolo::Tamanho);
        }
        let (chave, assinatura) = bytes.split_at(BYTES_CHAVE);
        Ok(Self {
            chave: chave.try_into().map_err(|_| ErroProtocolo::Tamanho)?,
            assinatura: assinatura.try_into().map_err(|_| ErroProtocolo::Tamanho)?,
        })
    }
}

impl fmt::Debug for ChaveAssinada {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChaveAssinada(oculta)")
    }
}

/// Uma publicação já conferida (ou ainda por assinar, do lado do dono).
#[derive(Clone, PartialEq, Eq)]
pub struct Publicacao {
    pub dono: [u8; BYTES_CHAVE],
    pub numero: u64,
    pub chaves: Vec<ChaveAssinada>,
}

impl fmt::Debug for Publicacao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Publicacao(numero {}, {} chaves, dono oculto)",
            self.numero,
            self.chaves.len()
        )
    }
}

impl Publicacao {
    fn corpo(&self) -> Result<Vec<u8>, ErroProtocolo> {
        if self.chaves.len() > MAXIMO_POR_PUBLICACAO {
            return Err(ErroProtocolo::Demais);
        }
        let n = u16::try_from(self.chaves.len()).map_err(|_| ErroProtocolo::Demais)?;
        let mut c = Vec::with_capacity(BYTES_CABECALHO + self.chaves.len() * BYTES_CHAVE_ASSINADA);
        c.push(VERSAO);
        c.extend_from_slice(&self.dono);
        c.extend_from_slice(&self.numero.to_le_bytes());
        c.extend_from_slice(&n.to_le_bytes());
        for k in &self.chaves {
            c.extend_from_slice(&k.para_bytes());
        }
        Ok(c)
    }

    /// O que o dono assina: o domínio seguido do corpo.
    pub fn para_assinar(&self) -> Result<Vec<u8>, ErroProtocolo> {
        let corpo = self.corpo()?;
        let mut m = Vec::with_capacity(DOMINIO_PUBLICACAO.len() + corpo.len());
        m.extend_from_slice(DOMINIO_PUBLICACAO);
        m.extend_from_slice(&corpo);
        Ok(m)
    }

    /// Bytes que vão para o relay: corpo e assinatura.
    pub fn codificar(&self, assinatura: &[u8; BYTES_ASSINATURA]) -> Result<Vec<u8>, ErroProtocolo> {
        let mut b = self.corpo()?;
        b.extend_from_slice(assinatura);
        Ok(b)
    }

    /// Lê sem conferir a assinatura: devolve a publicação e a assinatura que veio.
    pub fn ler(bytes: &[u8]) -> Result<(Self, [u8; BYTES_ASSINATURA]), ErroProtocolo> {
        if bytes.is_empty() {
            return Err(ErroProtocolo::Tamanho);
        }
        if bytes[0] != VERSAO {
            return Err(ErroProtocolo::Versao);
        }
        if bytes.len() < BYTES_MINIMO_PUBLICACAO {
            return Err(ErroProtocolo::Tamanho);
        }
        let dono: [u8; BYTES_CHAVE] = bytes[1..1 + BYTES_CHAVE]
            .try_into()
            .map_err(|_| ErroProtocolo::Tamanho)?;
        let numero = u64::from_le_bytes(
            bytes[1 + BYTES_CHAVE..1 + BYTES_CHAVE + 8]
                .try_into()
                .map_err(|_| ErroProtocolo::Tamanho)?,
        );
        let n = usize::from(u16::from_le_bytes(
            bytes[BYTES_CABECALHO - 2..BYTES_CABECALHO]
                .try_into()
                .map_err(|_| ErroProtocolo::Tamanho)?,
        ));
        if n > MAXIMO_POR_PUBLICACAO {
            return Err(ErroProtocolo::Demais);
        }
        if bytes.len() != BYTES_MINIMO_PUBLICACAO + n * BYTES_CHAVE_ASSINADA {
            return Err(ErroProtocolo::Tamanho);
        }
        let fim_chaves = BYTES_CABECALHO + n * BYTES_CHAVE_ASSINADA;
        // O tamanho já foi conferido: o trecho tem n pedaços inteiros e nenhuma sobra.
        let (pedacos, sobra) =
            bytes[BYTES_CABECALHO..fim_chaves].as_chunks::<BYTES_CHAVE_ASSINADA>();
        if !sobra.is_empty() {
            return Err(ErroProtocolo::Tamanho);
        }
        let chaves = pedacos
            .iter()
            .map(|p| ChaveAssinada::de_bytes(p))
            .collect::<Result<Vec<_>, _>>()?;
        let assinatura: [u8; BYTES_ASSINATURA] = bytes[fim_chaves..]
            .try_into()
            .map_err(|_| ErroProtocolo::Tamanho)?;
        Ok((
            Self {
                dono,
                numero,
                chaves,
            },
            assinatura,
        ))
    }

    /// Lê e confere a assinatura do dono. É o que o relay faz antes de aceitar.
    pub fn conferir(bytes: &[u8]) -> Result<Self, ErroProtocolo> {
        let (publicacao, assinatura) = Self::ler(bytes)?;
        let dono = Ed25519PublicKey::from_slice(&publicacao.dono)
            .map_err(|_| ErroProtocolo::ChaveInvalida)?;
        let assinatura =
            Ed25519Signature::from_slice(&assinatura).map_err(|_| ErroProtocolo::Assinatura)?;
        dono.verify(&publicacao.para_assinar()?, &assinatura)
            .map_err(|_| ErroProtocolo::Assinatura)?;
        Ok(publicacao)
    }
}

/// O dono no caminho do pedido: 64 caracteres hexadecimais minúsculos.
pub fn dono_em_texto(dono: &[u8; BYTES_CHAVE]) -> String {
    dono.iter().map(|b| format!("{b:02x}")).collect()
}

/// O inverso de `dono_em_texto`. Estrito: uma grafia só por dono.
pub fn dono_de_texto(texto: &str) -> Result<[u8; BYTES_CHAVE], ErroProtocolo> {
    let b = texto.as_bytes();
    if b.len() != 2 * BYTES_CHAVE {
        return Err(ErroProtocolo::Dono);
    }
    let valor = |c: u8| match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        _ => Err(ErroProtocolo::Dono),
    };
    let mut dono = [0u8; BYTES_CHAVE];
    for (i, par) in b.as_chunks::<2>().0.iter().enumerate() {
        dono[i] = (valor(par[0])? << 4) | valor(par[1])?;
    }
    Ok(dono)
}

#[cfg(test)]
mod testes {
    use super::*;

    type R = Result<(), Box<dyn std::error::Error>>;

    fn de_hex(texto: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        (0..texto.len())
            .step_by(2)
            .map(|i| {
                Ok(u8::from_str_radix(
                    texto.get(i..i + 2).ok_or("hex ímpar")?,
                    16,
                )?)
            })
            .collect()
    }

    // ---------- vetor conhecido (regra 8.4): ferramentas/oraculos/publicacao.py ----------

    /// Ed25519 pública do teste 1 da RFC 8032; a secreta assinou fora do Rust.
    const DONO: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
    const VETOR_PUBLICACAO: &str = include_str!("vetor_publicacao.hex");
    const VETOR_CONSULTA: &str = include_str!("vetor_consulta.hex");

    #[test]
    fn vetor_conhecido_da_publicacao_confere() -> R {
        let bytes = de_hex(VETOR_PUBLICACAO.trim())?;
        let p = Publicacao::conferir(&bytes)?;
        assert_eq!(dono_em_texto(&p.dono), DONO);
        assert_eq!(p.numero, 7);
        assert_eq!(p.chaves.len(), 2);
        assert_eq!(p.chaves[0].chave, [0x11; 32]);
        assert_eq!(p.chaves[0].assinatura, [0x22; 64]);
        assert_eq!(p.chaves[1].chave, [0x33; 32]);
        assert_eq!(p.chaves[1].assinatura, [0x44; 64]);
        // Ida e volta: montar de novo com a assinatura lida dá os mesmos bytes.
        let (_, assinatura) = Publicacao::ler(&bytes)?;
        assert_eq!(p.codificar(&assinatura)?, bytes);
        Ok(())
    }

    #[test]
    fn vetor_conhecido_da_consulta_confere() -> R {
        let bytes = de_hex(VETOR_CONSULTA.trim())?;
        assert_eq!(bytes.len(), BYTES_MINIMO_PUBLICACAO);
        let p = Publicacao::conferir(&bytes)?;
        assert_eq!((p.numero, p.chaves.len()), (u64::MAX, 0));
        Ok(())
    }

    /// Adulteração (regra 8.4): cada bit de cada byte trocado é recusado.
    #[test]
    fn qualquer_bit_trocado_e_recusado() -> R {
        let original = de_hex(VETOR_PUBLICACAO.trim())?;
        let mut por_motivo = std::collections::BTreeMap::<&str, usize>::new();
        for posicao in 0..original.len() {
            for bit in 0..8 {
                let mut b = original.clone();
                b[posicao] ^= 1 << bit;
                match Publicacao::conferir(&b) {
                    Ok(_) => return Err(format!("byte {posicao}, bit {bit} passou").into()),
                    Err(e) => *por_motivo.entry(e.codigo()).or_default() += 1,
                }
            }
        }
        let total: usize = por_motivo.values().sum();
        assert_eq!(total, original.len() * 8);
        assert_eq!(por_motivo.get("PROTOCOLO_VERSAO"), Some(&8));
        Ok(())
    }

    #[test]
    fn cortada_ou_com_byte_a_mais_e_recusada() -> R {
        let original = de_hex(VETOR_PUBLICACAO.trim())?;
        for n in 0..original.len() {
            assert!(
                Publicacao::ler(&original[..n]).is_err(),
                "cortada em {n} passou"
            );
        }
        let mut longa = original.clone();
        longa.push(0);
        assert_eq!(Publicacao::ler(&longa).err(), Some(ErroProtocolo::Tamanho));
        Ok(())
    }

    #[test]
    fn mais_que_o_maximo_e_recusado_dos_dois_lados() -> R {
        let demais = Publicacao {
            dono: [1; 32],
            numero: 1,
            chaves: vec![
                ChaveAssinada {
                    chave: [0; 32],
                    assinatura: [0; 64],
                };
                MAXIMO_POR_PUBLICACAO + 1
            ],
        };
        assert_eq!(demais.para_assinar().err(), Some(ErroProtocolo::Demais));
        // Do lado de quem lê: n declarado acima do máximo, antes de olhar o tamanho.
        let mut b = de_hex(VETOR_CONSULTA.trim())?;
        let n = u16::try_from(MAXIMO_POR_PUBLICACAO + 1)?;
        b[BYTES_CABECALHO - 2..BYTES_CABECALHO].copy_from_slice(&n.to_le_bytes());
        assert_eq!(Publicacao::ler(&b).err(), Some(ErroProtocolo::Demais));
        assert_eq!(
            BYTES_MAXIMO_PUBLICACAO,
            107 + 100 * 96,
            "limite do corpo no relay"
        );
        Ok(())
    }

    #[test]
    fn assinatura_de_outra_publicacao_e_recusada() -> R {
        let (p, assinatura) = Publicacao::ler(&de_hex(VETOR_PUBLICACAO.trim())?)?;
        // Mesmo conteúdo com outro número, assinatura antiga: é a repetição que o
        // número existe para impedir, e ela não passa nem na assinatura.
        let outra = Publicacao {
            numero: p.numero + 1,
            ..p.clone()
        };
        assert_eq!(
            Publicacao::conferir(&outra.codificar(&assinatura)?).err(),
            Some(ErroProtocolo::Assinatura)
        );
        Ok(())
    }

    #[test]
    fn chave_assinada_tem_96_bytes_exatos() -> R {
        let k = ChaveAssinada {
            chave: [7; 32],
            assinatura: [9; 64],
        };
        assert_eq!(ChaveAssinada::de_bytes(&k.para_bytes())?, k);
        for n in [0usize, 95, 97] {
            assert_eq!(
                ChaveAssinada::de_bytes(&vec![0; n]).err(),
                Some(ErroProtocolo::Tamanho)
            );
        }
        Ok(())
    }

    #[test]
    fn dono_em_texto_e_estrito() -> R {
        let dono: [u8; 32] = de_hex(DONO)?.try_into().map_err(|_| "dono")?;
        assert_eq!(dono_de_texto(&dono_em_texto(&dono))?, dono);
        for ruim in [
            "",
            &DONO[..63],
            &DONO.to_uppercase(),
            &format!("{DONO}0"),
            &format!("{}g", &DONO[..63]),
            &format!(" {}", &DONO[1..]),
        ] {
            assert_eq!(
                dono_de_texto(ruim).err(),
                Some(ErroProtocolo::Dono),
                "{ruim:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn debug_nao_mostra_chave() -> R {
        let (p, _) = Publicacao::ler(&de_hex(VETOR_PUBLICACAO.trim())?)?;
        let d = format!("{p:?} {:?}", p.chaves[0]);
        assert!(!d.contains(&DONO[..16]) && !d.contains("1111") && !d.contains("17, 17"));
        assert_eq!(
            d,
            "Publicacao(numero 7, 2 chaves, dono oculto) ChaveAssinada(oculta)"
        );
        Ok(())
    }
}
