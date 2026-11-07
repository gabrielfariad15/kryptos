//! Malote: as cartas esperando o destinatário no relay
//! (documentacao/arquitetura/malote.md).
//!
//! Depósito: o corpo do pedido é a própria carta (`carta`). Nada de quem manda.
//!
//! Busca, que só o destinatário faz, assinada com a Ed25519 dele:
//!
//! ```text
//! corpo = versão (1) || destinatário (32, Ed25519) || número (8, u64 LE)
//!         || n (2, u16 LE) || n × id (16)
//! busca = corpo || assinatura (64) = Ed25519 do destinatário sobre DOMINIO_BUSCA || corpo
//! ```
//!
//! - Os `n` ids são as cartas da entrega anterior que já foram guardadas no aparelho:
//!   o relay as apaga antes de entregar as próximas. Confirmar e buscar é um pedido só.
//! - O número só sobe, como na publicação: uma busca copiada não pode ser repetida.
//! - Leitura estrita: tamanho exato, sem id repetido.
//!
//! Entrega (resposta da busca):
//!
//! ```text
//! entrega = n (2, u16 LE) || n × (id (16) || tamanho (4, u32 LE) || carta)
//! ```
//!
//! Regra 8.2: o `Debug` não mostra destinatário, ids nem cartas.

use std::fmt;

use vodozemac::{Ed25519PublicKey, Ed25519Signature};

use crate::carta::{BYTES_CABECALHO_CARTA, MAXIMO_MENSAGEM};
use crate::{BYTES_ASSINATURA, BYTES_CHAVE, ErroProtocolo, VERSAO};

/// Separa a assinatura da busca de qualquer outra coisa que a mesma Ed25519 assinar.
pub const DOMINIO_BUSCA: &[u8] = b"KRYPTOS/busca/v1";

/// Identificador que o relay dá a cada carta guardada.
pub const BYTES_ID: usize = 16;

/// Ids confirmados numa busca. Estimo: cobre uma entrega cheia de cartas pequenas.
pub const MAXIMO_CONFIRMADAS: usize = 500;

/// Maior carta que o malote guarda: cabeçalho e a maior mensagem Olm aceita.
pub const BYTES_MAXIMO_CARTA: usize = BYTES_CABECALHO_CARTA + MAXIMO_MENSAGEM;

/// Maior entrega numa resposta. Estimo: 1 MiB, cerca de 15 cartas do tamanho máximo
/// ou milhares das pequenas. O aparelho busca de novo até vir vazia.
pub const BYTES_MAXIMO_ENTREGA: usize = 1 << 20;

const BYTES_CABECALHO_BUSCA: usize = 1 + BYTES_CHAVE + 8 + 2;
/// Busca sem confirmação.
pub const BYTES_MINIMO_BUSCA: usize = BYTES_CABECALHO_BUSCA + BYTES_ASSINATURA;
/// Maior busca aceita. O relay limita o corpo do pedido por este número.
pub const BYTES_MAXIMO_BUSCA: usize = BYTES_MINIMO_BUSCA + MAXIMO_CONFIRMADAS * BYTES_ID;

const BYTES_CABECALHO_ITEM: usize = BYTES_ID + 4;

#[derive(Clone, PartialEq, Eq)]
pub struct Busca {
    pub destinatario: [u8; BYTES_CHAVE],
    pub numero: u64,
    pub confirmadas: Vec<[u8; BYTES_ID]>,
}

impl fmt::Debug for Busca {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Busca(numero {}, {} confirmadas, destinatario oculto)",
            self.numero,
            self.confirmadas.len()
        )
    }
}

impl Busca {
    fn corpo(&self) -> Result<Vec<u8>, ErroProtocolo> {
        if self.confirmadas.len() > MAXIMO_CONFIRMADAS {
            return Err(ErroProtocolo::Demais);
        }
        let n = u16::try_from(self.confirmadas.len()).map_err(|_| ErroProtocolo::Demais)?;
        let mut c = Vec::with_capacity(BYTES_CABECALHO_BUSCA + self.confirmadas.len() * BYTES_ID);
        c.push(VERSAO);
        c.extend_from_slice(&self.destinatario);
        c.extend_from_slice(&self.numero.to_le_bytes());
        c.extend_from_slice(&n.to_le_bytes());
        for id in &self.confirmadas {
            c.extend_from_slice(id);
        }
        Ok(c)
    }

    /// O que o destinatário assina: o domínio seguido do corpo.
    pub fn para_assinar(&self) -> Result<Vec<u8>, ErroProtocolo> {
        let corpo = self.corpo()?;
        let mut m = Vec::with_capacity(DOMINIO_BUSCA.len() + corpo.len());
        m.extend_from_slice(DOMINIO_BUSCA);
        m.extend_from_slice(&corpo);
        Ok(m)
    }

    /// Bytes que vão para o relay: corpo e assinatura.
    pub fn codificar(&self, assinatura: &[u8; BYTES_ASSINATURA]) -> Result<Vec<u8>, ErroProtocolo> {
        let mut b = self.corpo()?;
        b.extend_from_slice(assinatura);
        Ok(b)
    }

    /// Lê sem conferir a assinatura.
    pub fn ler(bytes: &[u8]) -> Result<(Self, [u8; BYTES_ASSINATURA]), ErroProtocolo> {
        if bytes.is_empty() {
            return Err(ErroProtocolo::Tamanho);
        }
        if bytes[0] != VERSAO {
            return Err(ErroProtocolo::Versao);
        }
        if bytes.len() < BYTES_MINIMO_BUSCA {
            return Err(ErroProtocolo::Tamanho);
        }
        let destinatario: [u8; BYTES_CHAVE] = bytes[1..1 + BYTES_CHAVE]
            .try_into()
            .map_err(|_| ErroProtocolo::Tamanho)?;
        let numero = u64::from_le_bytes(
            bytes[1 + BYTES_CHAVE..1 + BYTES_CHAVE + 8]
                .try_into()
                .map_err(|_| ErroProtocolo::Tamanho)?,
        );
        let n = usize::from(u16::from_le_bytes(
            bytes[BYTES_CABECALHO_BUSCA - 2..BYTES_CABECALHO_BUSCA]
                .try_into()
                .map_err(|_| ErroProtocolo::Tamanho)?,
        ));
        if n > MAXIMO_CONFIRMADAS {
            return Err(ErroProtocolo::Demais);
        }
        if bytes.len() != BYTES_MINIMO_BUSCA + n * BYTES_ID {
            return Err(ErroProtocolo::Tamanho);
        }
        let fim = BYTES_CABECALHO_BUSCA + n * BYTES_ID;
        let (pedacos, sobra) = bytes[BYTES_CABECALHO_BUSCA..fim].as_chunks::<BYTES_ID>();
        if !sobra.is_empty() {
            return Err(ErroProtocolo::Tamanho);
        }
        let confirmadas: Vec<[u8; BYTES_ID]> = pedacos.to_vec();
        let mut ordenadas = confirmadas.clone();
        ordenadas.sort_unstable();
        if ordenadas.windows(2).any(|w| w[0] == w[1]) {
            return Err(ErroProtocolo::Repetida);
        }
        let assinatura: [u8; BYTES_ASSINATURA] = bytes[fim..]
            .try_into()
            .map_err(|_| ErroProtocolo::Tamanho)?;
        Ok((
            Self {
                destinatario,
                numero,
                confirmadas,
            },
            assinatura,
        ))
    }

    /// Lê e confere a assinatura do destinatário. É o que o relay faz antes de entregar.
    pub fn conferir(bytes: &[u8]) -> Result<Self, ErroProtocolo> {
        let (busca, assinatura) = Self::ler(bytes)?;
        let destinatario = Ed25519PublicKey::from_slice(&busca.destinatario)
            .map_err(|_| ErroProtocolo::ChaveInvalida)?;
        let assinatura =
            Ed25519Signature::from_slice(&assinatura).map_err(|_| ErroProtocolo::Assinatura)?;
        destinatario
            .verify(&busca.para_assinar()?, &assinatura)
            .map_err(|_| ErroProtocolo::Assinatura)?;
        Ok(busca)
    }
}

/// Cartas entregues numa busca, cada uma com o id que a confirma.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Entrega {
    pub cartas: Vec<([u8; BYTES_ID], Vec<u8>)>,
}

impl fmt::Debug for Entrega {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Entrega({} cartas)", self.cartas.len())
    }
}

impl Entrega {
    /// Quanto uma carta ocupa na entrega.
    pub fn bytes_do_item(carta: &[u8]) -> usize {
        BYTES_CABECALHO_ITEM + carta.len()
    }

    pub fn codificar(&self) -> Result<Vec<u8>, ErroProtocolo> {
        let n = u16::try_from(self.cartas.len()).map_err(|_| ErroProtocolo::Demais)?;
        let mut b = Vec::new();
        b.extend_from_slice(&n.to_le_bytes());
        for (id, carta) in &self.cartas {
            if carta.is_empty() || carta.len() > BYTES_MAXIMO_CARTA {
                return Err(ErroProtocolo::Tamanho);
            }
            let tamanho = u32::try_from(carta.len()).map_err(|_| ErroProtocolo::Tamanho)?;
            b.extend_from_slice(id);
            b.extend_from_slice(&tamanho.to_le_bytes());
            b.extend_from_slice(carta);
        }
        if b.len() > 2 + BYTES_MAXIMO_ENTREGA {
            return Err(ErroProtocolo::Demais);
        }
        Ok(b)
    }

    /// Leitura estrita: cada carta com tamanho entre 1 e `BYTES_MAXIMO_CARTA`, sem sobra
    /// no fim e sem id repetido. O conteúdo de cada carta o núcleo confere ao receber.
    pub fn ler(bytes: &[u8]) -> Result<Self, ErroProtocolo> {
        if bytes.len() < 2 || bytes.len() > 2 + BYTES_MAXIMO_ENTREGA {
            return Err(ErroProtocolo::Tamanho);
        }
        let n = usize::from(u16::from_le_bytes([bytes[0], bytes[1]]));
        let mut resto = &bytes[2..];
        let mut cartas = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            if resto.len() < BYTES_CABECALHO_ITEM {
                return Err(ErroProtocolo::Tamanho);
            }
            let (cabecalho, depois) = resto.split_at(BYTES_CABECALHO_ITEM);
            let id: [u8; BYTES_ID] = cabecalho[..BYTES_ID]
                .try_into()
                .map_err(|_| ErroProtocolo::Tamanho)?;
            let tamanho = u32::from_le_bytes(
                cabecalho[BYTES_ID..]
                    .try_into()
                    .map_err(|_| ErroProtocolo::Tamanho)?,
            );
            let tamanho = usize::try_from(tamanho).map_err(|_| ErroProtocolo::Tamanho)?;
            if tamanho == 0 || tamanho > BYTES_MAXIMO_CARTA || depois.len() < tamanho {
                return Err(ErroProtocolo::Tamanho);
            }
            let (carta, depois) = depois.split_at(tamanho);
            if cartas.iter().any(|(outro, _)| *outro == id) {
                return Err(ErroProtocolo::Repetida);
            }
            cartas.push((id, carta.to_vec()));
            resto = depois;
        }
        if !resto.is_empty() {
            return Err(ErroProtocolo::Tamanho);
        }
        Ok(Self { cartas })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const VETOR_BUSCA: &str = include_str!("vetor_busca.hex");

    fn hex(t: &str) -> Vec<u8> {
        t.trim()
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap_or("zz"), 16).unwrap_or(0))
            .collect()
    }

    /// Ed25519 pública do teste 1 do RFC 8032: o oráculo assina com a secreta dele.
    const DONO_RFC: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

    #[test]
    fn vetor_do_oraculo_confere() -> Result<(), ErroProtocolo> {
        let bytes = hex(VETOR_BUSCA);
        let b = Busca::conferir(&bytes)?;
        assert_eq!(b.destinatario.to_vec(), hex(DONO_RFC));
        assert_eq!(b.numero, 7);
        assert_eq!(b.confirmadas, vec![[0x11; BYTES_ID], [0x22; BYTES_ID]]);
        // Recodificar dá os mesmos bytes.
        let (lida, assinatura) = Busca::ler(&bytes)?;
        assert_eq!(lida.codificar(&assinatura)?, bytes);
        Ok(())
    }

    #[test]
    fn qualquer_bit_trocado_e_recusado() {
        let bytes = hex(VETOR_BUSCA);
        for i in 0..bytes.len() {
            for bit in 0..8 {
                let mut b = bytes.clone();
                b[i] ^= 1 << bit;
                assert!(Busca::conferir(&b).is_err(), "byte {i} bit {bit}");
            }
        }
    }

    #[test]
    fn tamanhos_errados_e_repetidas_sao_recusados() -> Result<(), ErroProtocolo> {
        let bytes = hex(VETOR_BUSCA);
        assert_eq!(Busca::ler(&[]).err(), Some(ErroProtocolo::Tamanho));
        assert_eq!(
            Busca::ler(&bytes[..bytes.len() - 1]).err(),
            Some(ErroProtocolo::Tamanho)
        );
        let mut maior = bytes.clone();
        maior.push(0);
        assert_eq!(Busca::ler(&maior).err(), Some(ErroProtocolo::Tamanho));
        let mut versao = bytes.clone();
        versao[0] = 2;
        assert_eq!(Busca::ler(&versao).err(), Some(ErroProtocolo::Versao));
        let b = Busca {
            destinatario: [1; 32],
            numero: 1,
            confirmadas: vec![[9; BYTES_ID], [9; BYTES_ID]],
        };
        let repetida = b.codificar(&[0; 64])?;
        assert_eq!(Busca::ler(&repetida).err(), Some(ErroProtocolo::Repetida));
        let demais = Busca {
            confirmadas: (0..=MAXIMO_CONFIRMADAS)
                .map(|i| {
                    let mut id = [0u8; BYTES_ID];
                    id[..8].copy_from_slice(&(i as u64).to_le_bytes());
                    id
                })
                .collect(),
            ..b
        };
        assert_eq!(
            demais.codificar(&[0; 64]).err(),
            Some(ErroProtocolo::Demais)
        );
        Ok(())
    }

    #[test]
    fn entrega_ida_e_volta_e_recusas() -> Result<(), ErroProtocolo> {
        let e = Entrega {
            cartas: vec![([1; BYTES_ID], vec![7; 100]), ([2; BYTES_ID], vec![8; 3])],
        };
        let b = e.codificar()?;
        assert_eq!(b.len(), 2 + 2 * BYTES_CABECALHO_ITEM + 103);
        assert_eq!(Entrega::ler(&b)?, e);
        assert_eq!(
            Entrega::ler(&Entrega::default().codificar()?)?,
            Entrega::default()
        );
        // cortada, com sobra, n maior que o conteúdo
        assert!(Entrega::ler(&b[..b.len() - 1]).is_err());
        let mut sobra = b.clone();
        sobra.push(0);
        assert!(Entrega::ler(&sobra).is_err());
        let mut n_maior = b.clone();
        n_maior[0] = 3;
        assert!(Entrega::ler(&n_maior).is_err());
        // id repetido
        let rep = Entrega {
            cartas: vec![([1; BYTES_ID], vec![7]), ([1; BYTES_ID], vec![8])],
        }
        .codificar()?;
        assert_eq!(Entrega::ler(&rep).err(), Some(ErroProtocolo::Repetida));
        // carta vazia e grande demais
        assert!(
            Entrega {
                cartas: vec![([1; BYTES_ID], vec![])]
            }
            .codificar()
            .is_err()
        );
        assert!(
            Entrega {
                cartas: vec![([1; BYTES_ID], vec![0; BYTES_MAXIMO_CARTA + 1])]
            }
            .codificar()
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn debug_nao_mostra_destinatario_nem_ids() {
        let b = Busca {
            destinatario: [0xAB; 32],
            numero: 5,
            confirmadas: vec![[0xCD; BYTES_ID]],
        };
        let e = Entrega {
            cartas: vec![([0xCD; BYTES_ID], vec![0xEF; 4])],
        };
        for t in [format!("{b:?}"), format!("{e:?}")] {
            assert!(
                !t.contains("171") && !t.contains("ab") && !t.contains("cd"),
                "{t}"
            );
        }
    }
}
