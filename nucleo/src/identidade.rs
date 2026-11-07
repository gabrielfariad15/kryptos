//! Identidade do aparelho: a conta Olm do vodozemac (par Ed25519 para assinar e par
//! Curve25519 para combinar chaves). A parte secreta nunca sai deste módulo.
//!
//! Nesta etapa a identidade vive só na memória. Gravar cifrada (PIN + Keystore) é o
//! próximo assunto; até lá, fechar o app perde a identidade e o código muda.

use std::fmt;

use vodozemac::olm::{Account, IdentityKeys};

use crate::codigo::CodigoContato;

pub struct Identidade {
    conta: Account,
}

impl Identidade {
    /// Gera uma identidade nova com o gerador aleatório do sistema.
    pub fn nova() -> Self {
        Self {
            conta: Account::new(),
        }
    }

    /// As duas chaves públicas. Não são segredo, mas também não vão para log (regra 8.2).
    pub fn chaves_publicas(&self) -> IdentityKeys {
        self.conta.identity_keys()
    }

    pub fn codigo_contato(&self) -> CodigoContato {
        CodigoContato::de_chaves(&self.chaves_publicas())
    }
}

impl fmt::Debug for Identidade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Identidade(oculta)")
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn identidade_nova_da_codigo_que_passa_na_leitura() -> Result<(), crate::ErroCodigo> {
        let identidade = Identidade::nova();
        let codigo = identidade.codigo_contato();
        assert_eq!(CodigoContato::ler(&codigo.como_texto())?, codigo);
        Ok(())
    }

    #[test]
    fn mesma_identidade_da_sempre_o_mesmo_codigo() {
        let identidade = Identidade::nova();
        assert_eq!(identidade.codigo_contato(), identidade.codigo_contato());
    }

    #[test]
    fn duas_identidades_tem_chaves_diferentes() {
        let a = Identidade::nova().chaves_publicas();
        let b = Identidade::nova().chaves_publicas();
        assert_ne!(a.ed25519, b.ed25519);
        assert_ne!(a.curve25519, b.curve25519);
    }

    #[test]
    fn debug_nao_mostra_chave_nem_codigo() {
        let identidade = Identidade::nova();
        let depuracao = format!("{identidade:?}");
        let chaves = identidade.chaves_publicas();
        let codigo = identidade.codigo_contato().como_texto();
        for proibido in [
            chaves.ed25519.to_base64(),
            chaves.curve25519.to_base64(),
            codigo,
        ] {
            assert!(!depuracao.contains(&proibido), "Debug mostra {proibido}");
        }
    }
}
