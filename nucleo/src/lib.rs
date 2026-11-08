//! Núcleo do Kryptos: o motor que roda no aparelho.
//!
//! - `identidade`: a conta Olm (vodozemac) do aparelho.
//! - `codigo`: o código de contato que sai da chave pública.
//! - `cartao`: o texto do QR, com a chave pública inteira.
//! - `contatos`: a agenda de contatos (até 5.000).
//! - `agenda_gravada`: a agenda no disco, cifrada com a chave do cofre, tamanho fixo.
//! - `caixa`: as mensagens recebidas, seladas em 100 gavetas de tamanho fixo.
//! - `qr`: a grade do QR do cartão e a leitura do quadro da câmera.
//! - `cofre`: a identidade gravada cifrada (PIN + Keystore).
//! - `aparelho`: guarda a identidade desta execução e conduz o cofre; a chave não
//!   atravessa a ponte.
//! - `sessao`: a conversa entre duas pessoas (Olm), só entre contatos conferidos por QR.
//!
//! A queima entra numa próxima etapa, com teste de vetor conhecido e teste de
//! adulteração (regra 8.4).

pub mod agenda_gravada;
pub mod aparelho;
pub mod caixa;
pub mod cartao;
pub mod codigo;
pub mod cofre;
pub mod contatos;
pub mod conversas;
pub mod identidade;
pub mod qr;
pub mod sessao;

/// O formato do relay e da carta, para a ponte não depender de outro crate.
pub use kryptos_protocolo as protocolo;

pub use cartao::{CartaoContato, ErroCartao};
pub use codigo::{CodigoContato, ErroCodigo};
pub use contatos::{Adicao, Conferencia, Contato, ErroAgenda};
pub use qr::{ErroQr, GradeQr};
pub use sessao::{ChaveAvulsa, ErroSessao, Sessao};

/// Versão do núcleo, tirada do `Cargo.toml` na compilação.
pub fn versao() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn versao_e_a_do_cargo_toml() {
        assert_eq!(versao(), "0.1.0");
    }

    #[test]
    fn versao_tem_tres_partes_numericas() {
        let partes: Vec<&str> = versao().split('.').collect();
        assert_eq!(
            partes.len(),
            3,
            "versão fora do formato X.Y.Z: {}",
            versao()
        );
        for p in partes {
            assert!(p.parse::<u32>().is_ok(), "parte não numérica: {p}");
        }
    }
}
