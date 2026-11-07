//! Núcleo do Kryptos: o motor que roda no aparelho.
//!
//! - `identidade`: a conta Olm (vodozemac) do aparelho.
//! - `codigo`: o código de contato que sai da chave pública.
//! - `cofre`: a identidade gravada cifrada (PIN + Keystore).
//! - `aparelho`: guarda a identidade desta execução e conduz o cofre; a chave não
//!   atravessa a ponte.
//!
//! Protocolo e queima entram nas próximas etapas, cada uma com teste de vetor
//! conhecido e teste de adulteração (regra 8.4).

pub mod aparelho;
pub mod codigo;
pub mod cofre;
pub mod identidade;

pub use codigo::{CodigoContato, ErroCodigo};

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
