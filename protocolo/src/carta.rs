//! Carta: a mensagem cifrada como viaja, por qualquer caminho
//! (documentacao/arquitetura/transportes.md).
//!
//! A mesma carta vai pelo relay (internet) ou pelo modo Direto (Bluetooth, Wi-Fi
//! Direct, som, rádio LoRa). O núcleo recebe cartas por uma porta só e não sabe por
//! onde vieram.
//!
//! ```text
//! carta = versão (1) || destinatário (32, Ed25519) || tipo (1) || mensagem Olm (resto)
//!   tipo: 0 = primeira mensagem da conversa (PreKey), 1 = mensagem comum
//! ```
//!
//! - Quem manda não vai na carta: nem o relay nem quem escuta o rádio sabem. O
//!   destinatário descobre ao decifrar.
//! - Não há campo de tamanho: a carta é a sequência inteira de bytes. Dividir em
//!   pedaços (rádio) é do transporte, que remonta antes de entregar ao núcleo.
//! - Leitura estrita: versão, tipo e tamanho conferidos; mensagem vazia é recusada.
//!
//! Regra 8.2: o `Debug` não mostra destinatário nem mensagem.

use std::fmt;

use crate::{BYTES_CHAVE, ErroProtocolo};

pub const VERSAO_CARTA: u8 = 1;

/// Bytes antes da mensagem: versão, destinatário e tipo.
pub const BYTES_CABECALHO_CARTA: usize = 1 + BYTES_CHAVE + 1;

/// Maior mensagem Olm aceita numa carta. Estimo: cobre um texto de 64 KiB cifrado,
/// com folga para o cabeçalho do Olm.
pub const MAXIMO_MENSAGEM: usize = 65_536 + 1_024;

/// Tipo da mensagem Olm, igual ao do vodozemac (`OlmMessage::to_parts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    Primeira,
    Normal,
}

impl Tipo {
    pub fn numero(self) -> u8 {
        match self {
            Self::Primeira => 0,
            Self::Normal => 1,
        }
    }

    pub fn de_numero(n: u8) -> Result<Self, ErroProtocolo> {
        match n {
            0 => Ok(Self::Primeira),
            1 => Ok(Self::Normal),
            _ => Err(ErroProtocolo::Tipo),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Carta {
    pub destinatario: [u8; BYTES_CHAVE],
    pub tipo: Tipo,
    pub mensagem: Vec<u8>,
}

impl fmt::Debug for Carta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Carta({:?}, {} bytes, destinatário oculto)",
            self.tipo,
            self.mensagem.len()
        )
    }
}

impl Carta {
    pub fn codificar(&self) -> Result<Vec<u8>, ErroProtocolo> {
        if self.mensagem.is_empty() || self.mensagem.len() > MAXIMO_MENSAGEM {
            return Err(ErroProtocolo::Tamanho);
        }
        let mut b = Vec::with_capacity(BYTES_CABECALHO_CARTA + self.mensagem.len());
        b.push(VERSAO_CARTA);
        b.extend_from_slice(&self.destinatario);
        b.push(self.tipo.numero());
        b.extend_from_slice(&self.mensagem);
        Ok(b)
    }

    pub fn ler(bytes: &[u8]) -> Result<Self, ErroProtocolo> {
        let (&versao, resto) = bytes.split_first().ok_or(ErroProtocolo::Tamanho)?;
        if versao != VERSAO_CARTA {
            return Err(ErroProtocolo::Versao);
        }
        if bytes.len() <= BYTES_CABECALHO_CARTA
            || bytes.len() - BYTES_CABECALHO_CARTA > MAXIMO_MENSAGEM
        {
            return Err(ErroProtocolo::Tamanho);
        }
        let (destinatario, resto) = resto.split_at(BYTES_CHAVE);
        let (&tipo, mensagem) = resto.split_first().ok_or(ErroProtocolo::Tamanho)?;
        Ok(Self {
            destinatario: destinatario
                .try_into()
                .map_err(|_| ErroProtocolo::Tamanho)?,
            tipo: Tipo::de_numero(tipo)?,
            mensagem: mensagem.to_vec(),
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    type R = Result<(), Box<dyn std::error::Error>>;

    fn exemplo() -> Carta {
        Carta {
            destinatario: [0xAB; 32],
            tipo: Tipo::Primeira,
            mensagem: vec![3, 10, 32, 7, 7, 7],
        }
    }

    #[test]
    fn vai_e_volta_nos_dois_tipos() -> R {
        for tipo in [Tipo::Primeira, Tipo::Normal] {
            let c = Carta { tipo, ..exemplo() };
            let b = c.codificar()?;
            assert_eq!(b.len(), BYTES_CABECALHO_CARTA + 6);
            assert_eq!(b[0], VERSAO_CARTA);
            assert_eq!(b[33], tipo.numero());
            assert_eq!(Carta::ler(&b)?, c);
        }
        Ok(())
    }

    /// Bytes fixos: o formato não muda sem trocar a versão.
    #[test]
    fn formato_fixo() -> R {
        let mut esperado = vec![1u8];
        esperado.extend_from_slice(&[0xAB; 32]);
        esperado.push(0);
        esperado.extend_from_slice(&[3, 10, 32, 7, 7, 7]);
        assert_eq!(exemplo().codificar()?, esperado);
        Ok(())
    }

    #[test]
    fn versao_tipo_e_tamanho_errados_sao_recusados() -> R {
        let boa = exemplo().codificar()?;
        let mut v = boa.clone();
        v[0] = 2;
        assert_eq!(Carta::ler(&v).err(), Some(ErroProtocolo::Versao));
        for tipo in [2u8, 0xFF] {
            let mut t = boa.clone();
            t[33] = tipo;
            assert_eq!(Carta::ler(&t).err(), Some(ErroProtocolo::Tipo));
        }
        // Cortada em cada posição até o cabeçalho inteiro: sem mensagem não há carta.
        for n in 0..=BYTES_CABECALHO_CARTA {
            assert!(Carta::ler(&boa[..n]).is_err(), "cortada em {n} passou");
        }
        assert_eq!(Carta::ler(&[]).err(), Some(ErroProtocolo::Tamanho));
        Ok(())
    }

    #[test]
    fn mensagem_vazia_ou_grande_demais_e_recusada() -> R {
        let vazia = Carta {
            mensagem: Vec::new(),
            ..exemplo()
        };
        assert_eq!(vazia.codificar().err(), Some(ErroProtocolo::Tamanho));
        let no_limite = Carta {
            mensagem: vec![1; MAXIMO_MENSAGEM],
            ..exemplo()
        };
        let b = no_limite.codificar()?;
        assert_eq!(Carta::ler(&b)?.mensagem.len(), MAXIMO_MENSAGEM);
        let demais = Carta {
            mensagem: vec![1; MAXIMO_MENSAGEM + 1],
            ..exemplo()
        };
        assert_eq!(demais.codificar().err(), Some(ErroProtocolo::Tamanho));
        let mut b = b;
        b.push(1);
        assert_eq!(Carta::ler(&b).err(), Some(ErroProtocolo::Tamanho));
        Ok(())
    }

    #[test]
    fn codigos_de_erro_do_protocolo_sao_distintos() {
        let todos = [
            ErroProtocolo::Versao,
            ErroProtocolo::Tamanho,
            ErroProtocolo::Demais,
            ErroProtocolo::ChaveInvalida,
            ErroProtocolo::Assinatura,
            ErroProtocolo::Dono,
            ErroProtocolo::Tipo,
        ]
        .map(|e| e.codigo());
        let mut u = todos.to_vec();
        u.sort_unstable();
        u.dedup();
        assert_eq!(u.len(), todos.len());
    }

    #[test]
    fn debug_nao_mostra_destinatario_nem_mensagem() {
        let d = format!("{:?}", exemplo());
        assert_eq!(d, "Carta(Primeira, 6 bytes, destinatário oculto)");
    }
}
