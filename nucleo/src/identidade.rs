//! Identidade do aparelho: a conta Olm do vodozemac (par Ed25519 para assinar e par
//! Curve25519 para combinar chaves). A parte secreta só sai deste módulo cifrada, pelo
//! `cofre`.

use std::fmt;

use serde::Deserialize;
use vodozemac::olm::{Account, IdentityKeys};
use zeroize::Zeroizing;

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

    pub(crate) fn conta(&self) -> &Account {
        &self.conta
    }

    pub(crate) fn de_conta(conta: Account) -> Self {
        Self { conta }
    }

    /// As duas chaves secretas, só para a varredura que confere que nenhuma foi parar
    /// no disco em claro. Ficam em `Zeroizing`; a conta em JSON também.
    pub fn segredos_para_varredura(&self) -> Result<Vec<Zeroizing<Vec<u8>>>, serde_json::Error> {
        #[derive(Deserialize)]
        enum Assinatura {
            Normal(Vec<u8>),
            Expanded(Vec<u8>),
        }
        #[derive(Deserialize)]
        struct Segredos {
            signing_key: Assinatura,
            diffie_hellman_key: Vec<u8>,
        }
        let json = Zeroizing::new(serde_json::to_vec(&self.conta.pickle())?);
        let s: Segredos = serde_json::from_slice(&json)?;
        let assinatura = match s.signing_key {
            Assinatura::Normal(b) | Assinatura::Expanded(b) => b,
        };
        Ok(vec![
            Zeroizing::new(assinatura),
            Zeroizing::new(s.diffie_hellman_key),
        ])
    }

    #[cfg(test)]
    pub(crate) fn de_pickle_json(json: &str) -> Result<Self, serde_json::Error> {
        let pickle: vodozemac::olm::AccountPickle = serde_json::from_str(json)?;
        Ok(Self::de_conta(Account::from_pickle(pickle)))
    }

    #[cfg(test)]
    pub(crate) fn pickle_em_claro_para_teste(
        &self,
    ) -> Result<Zeroizing<Vec<u8>>, serde_json::Error> {
        Ok(Zeroizing::new(serde_json::to_vec(&self.conta.pickle())?))
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
    fn segredos_sao_32_bytes_e_diferentes_das_chaves_publicas() -> Result<(), serde_json::Error> {
        let identidade = Identidade::nova();
        let segredos = identidade.segredos_para_varredura()?;
        assert_eq!(segredos.len(), 2);
        let publicas = identidade.chaves_publicas();
        for s in &segredos {
            assert_eq!(s.len(), 32);
            assert_ne!(s.as_slice(), publicas.ed25519.as_bytes());
            assert_ne!(s.as_slice(), publicas.curve25519.as_bytes());
        }
        assert_ne!(segredos[0].as_slice(), segredos[1].as_slice());
        Ok(())
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
