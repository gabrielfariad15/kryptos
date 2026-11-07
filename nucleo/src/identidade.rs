//! Identidade do aparelho: a conta Olm do vodozemac (par Ed25519 para assinar e par
//! Curve25519 para combinar chaves). A parte secreta só sai deste módulo cifrada, pelo
//! `cofre`.

use std::collections::BTreeMap;
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

    /// Para gerar chave avulsa e abrir a sessão de quem escreveu primeiro
    /// (`sessao`), que mudam a conta.
    pub(crate) fn conta_mut(&mut self) -> &mut Account {
        &mut self.conta
    }

    pub(crate) fn de_conta(conta: Account) -> Self {
        Self { conta }
    }

    /// As chaves secretas, só para a varredura que confere que nenhuma foi parar no
    /// disco em claro: a de assinatura, a de combinar chaves e a secreta de cada chave
    /// avulsa ainda não gasta (`sessao`). Ficam em `Zeroizing`; a conta em JSON também.
    pub fn segredos_para_varredura(&self) -> Result<Vec<Zeroizing<Vec<u8>>>, serde_json::Error> {
        #[derive(Deserialize)]
        enum Assinatura {
            Normal(Vec<u8>),
            Expanded(Vec<u8>),
        }
        #[derive(Deserialize)]
        struct Avulsas {
            private_keys: BTreeMap<String, Vec<u8>>,
        }
        #[derive(Deserialize)]
        struct Segredos {
            signing_key: Assinatura,
            diffie_hellman_key: Vec<u8>,
            one_time_keys: Avulsas,
        }
        let json = Zeroizing::new(serde_json::to_vec(&self.conta.pickle())?);
        let s: Segredos = serde_json::from_slice(&json)?;
        let assinatura = match s.signing_key {
            Assinatura::Normal(b) | Assinatura::Expanded(b) => b,
        };
        let mut segredos = vec![
            Zeroizing::new(assinatura),
            Zeroizing::new(s.diffie_hellman_key),
        ];
        segredos.extend(
            s.one_time_keys
                .private_keys
                .into_values()
                .map(Zeroizing::new),
        );
        Ok(segredos)
    }

    /// Mesma conta, com as mesmas chaves avulsas: compara as duas contas inteiras em
    /// JSON (os mapas da conta são ordenados, então o JSON é estável). Serve para
    /// provar, lendo de volta, que o cofre gravado é a conta da memória.
    pub(crate) fn mesma_conta_que(&self, outra: &Identidade) -> Result<bool, serde_json::Error> {
        let a = Zeroizing::new(serde_json::to_vec(&self.conta.pickle())?);
        let b = Zeroizing::new(serde_json::to_vec(&outra.conta.pickle())?);
        Ok(a.as_slice() == b.as_slice())
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
    fn chaves_avulsas_entram_na_varredura_e_a_gasta_sai() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut identidade = Identidade::nova();
        assert_eq!(identidade.segredos_para_varredura()?.len(), 2);
        let avulsas = [
            identidade.nova_chave_avulsa()?,
            identidade.nova_chave_avulsa()?,
            identidade.nova_chave_avulsa()?,
        ];
        let segredos = identidade.segredos_para_varredura()?;
        assert_eq!(segredos.len(), 5);
        // Cada secreta corresponde a uma das públicas entregues, e nenhuma é a pública.
        let publicas: Vec<[u8; 32]> = avulsas.iter().map(|a| a.chave_bytes()).collect();
        for s in &segredos[2..] {
            let secreta: [u8; 32] = s.as_slice().try_into()?;
            let publica = vodozemac::Curve25519PublicKey::from(
                &vodozemac::Curve25519SecretKey::from_slice(&secreta),
            )
            .to_bytes();
            assert!(publicas.contains(&publica), "secreta sem pública entregue");
            assert!(!publicas.contains(&secreta));
        }
        Ok(())
    }

    #[test]
    fn mesma_conta_so_com_as_mesmas_chaves_avulsas() -> Result<(), Box<dyn std::error::Error>> {
        let mut a = Identidade::nova();
        let copia = Identidade::de_pickle_json(&String::from_utf8(
            a.pickle_em_claro_para_teste()?.to_vec(),
        )?)?;
        assert!(a.mesma_conta_que(&copia)?);
        a.nova_chave_avulsa()?;
        assert!(
            !a.mesma_conta_que(&copia)?,
            "chave avulsa nova não mudou a conta"
        );
        assert!(!a.mesma_conta_que(&Identidade::nova())?);
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
