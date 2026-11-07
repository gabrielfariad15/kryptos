//! Agenda de contatos desta execução do app. Fica só em memória e some quando o app
//! fecha (decisão de 07/11/2026). Guardar cifrado em disco mexe no formato do cofre e
//! é assunto próprio.
//!
//! Um contato entra por um cartão lido do QR. A agenda recusa:
//! - o próprio cartão;
//! - um cartão em conflito com alguém que já está nela, ou com o próprio aparelho:
//!   chaves diferentes com o mesmo código de contato, ou com uma das duas chaves igual.
//!   O código tem só 35 bits, e gerar uma chave com o código de outra pessoa custa
//!   horas de computador. Por isso a agenda compara as chaves inteiras e trata código
//!   repetido como tentativa de imitação, nunca como "o mesmo contato".

use vodozemac::olm::IdentityKeys;

use crate::cartao::{CartaoContato, ErroCartao};
use crate::codigo::CodigoContato;

/// Como a chave do contato foi conferida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conferencia {
    /// O QR foi lido pessoalmente: o cartão traz a chave inteira.
    PorQr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Contato {
    cartao: CartaoContato,
    codigo: CodigoContato,
    conferencia: Conferencia,
}

impl Contato {
    fn conferido_por_qr(cartao: CartaoContato) -> Self {
        Self {
            codigo: cartao.codigo(),
            cartao,
            conferencia: Conferencia::PorQr,
        }
    }

    pub fn codigo(&self) -> CodigoContato {
        self.codigo
    }

    pub fn conferencia(&self) -> Conferencia {
        self.conferencia
    }

    pub fn chaves(&self) -> &IdentityKeys {
        self.cartao.chaves()
    }
}

/// O que aconteceu com um cartão aceito.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adicao {
    Novo(CodigoContato),
    /// Já estava na agenda com as mesmas chaves: nada muda.
    JaEstava(CodigoContato),
}

/// Motivo de um cartão não entrar na agenda. Não carrega o texto lido (regra 8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroAgenda {
    Cartao(ErroCartao),
    /// O QR é o deste aparelho.
    Proprio,
    /// Mesmo código ou uma das chaves igual a alguém da agenda (ou a este aparelho),
    /// com a outra chave diferente.
    Conflito,
    /// Não há identidade na memória para comparar.
    SemIdentidade,
}

impl ErroAgenda {
    pub fn codigo(&self) -> &'static str {
        match self {
            Self::Cartao(e) => e.codigo(),
            Self::Proprio => "QR_PROPRIO",
            Self::Conflito => "QR_CONFLITO",
            Self::SemIdentidade => "SEM_IDENTIDADE",
        }
    }
}

impl std::fmt::Display for ErroAgenda {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErroAgenda {}

impl From<ErroCartao> for ErroAgenda {
    fn from(e: ErroCartao) -> Self {
        Self::Cartao(e)
    }
}

#[derive(Debug, Default)]
pub struct Agenda {
    contatos: Vec<Contato>,
}

impl Agenda {
    pub const fn nova() -> Self {
        Self {
            contatos: Vec::new(),
        }
    }

    pub fn contatos(&self) -> &[Contato] {
        &self.contatos
    }

    pub fn limpar(&mut self) {
        self.contatos.clear();
    }

    /// Lê o texto do QR e põe o contato na agenda, conferido por QR.
    pub fn adicionar_por_qr(
        &mut self,
        texto: &str,
        proprio: &IdentityKeys,
    ) -> Result<Adicao, ErroAgenda> {
        let novo = Contato::conferido_por_qr(CartaoContato::ler(texto)?);
        let eu = Contato::conferido_por_qr(CartaoContato::de_chaves(proprio));
        if novo.chaves() == eu.chaves() {
            return Err(ErroAgenda::Proprio);
        }
        if em_conflito(&novo, &eu) {
            return Err(ErroAgenda::Conflito);
        }
        for existente in &self.contatos {
            if existente.chaves() == novo.chaves() {
                return Ok(Adicao::JaEstava(existente.codigo));
            }
            if em_conflito(&novo, existente) {
                return Err(ErroAgenda::Conflito);
            }
        }
        self.contatos.push(novo);
        Ok(Adicao::Novo(novo.codigo))
    }
}

/// Chaves diferentes que se parecem: mesmo código, ou uma das chaves igual.
fn em_conflito(a: &Contato, b: &Contato) -> bool {
    let (ka, kb) = (a.chaves(), b.chaves());
    ka != kb && (a.codigo == b.codigo || ka.ed25519 == kb.ed25519 || ka.curve25519 == kb.curve25519)
}

#[cfg(test)]
mod testes {
    use vodozemac::Curve25519PublicKey;

    use super::*;
    use crate::identidade::Identidade;

    fn texto_de(chaves: &IdentityKeys) -> String {
        CartaoContato::de_chaves(chaves).como_texto()
    }

    #[test]
    fn contato_novo_entra_conferido_por_qr() -> Result<(), ErroAgenda> {
        let eu = Identidade::nova().chaves_publicas();
        let outro = Identidade::nova();
        let mut agenda = Agenda::nova();
        let adicao = agenda.adicionar_por_qr(&texto_de(&outro.chaves_publicas()), &eu)?;
        assert_eq!(adicao, Adicao::Novo(outro.codigo_contato()));
        assert_eq!(agenda.contatos().len(), 1);
        let c = agenda.contatos()[0];
        assert_eq!(c.codigo(), outro.codigo_contato());
        assert_eq!(c.conferencia(), Conferencia::PorQr);
        assert_eq!(c.chaves(), &outro.chaves_publicas());
        Ok(())
    }

    #[test]
    fn ler_o_mesmo_qr_de_novo_nao_duplica() -> Result<(), ErroAgenda> {
        let eu = Identidade::nova().chaves_publicas();
        let outro = Identidade::nova();
        let texto = texto_de(&outro.chaves_publicas());
        let mut agenda = Agenda::nova();
        agenda.adicionar_por_qr(&texto, &eu)?;
        assert_eq!(
            agenda.adicionar_por_qr(&texto, &eu)?,
            Adicao::JaEstava(outro.codigo_contato())
        );
        assert_eq!(agenda.contatos().len(), 1);
        Ok(())
    }

    #[test]
    fn varios_contatos_distintos_entram_todos() -> Result<(), ErroAgenda> {
        let eu = Identidade::nova().chaves_publicas();
        let mut agenda = Agenda::nova();
        for _ in 0..50 {
            let outro = Identidade::nova().chaves_publicas();
            assert!(matches!(
                agenda.adicionar_por_qr(&texto_de(&outro), &eu)?,
                Adicao::Novo(_)
            ));
        }
        assert_eq!(agenda.contatos().len(), 50);
        agenda.limpar();
        assert!(agenda.contatos().is_empty());
        Ok(())
    }

    #[test]
    fn proprio_qr_e_recusado() {
        let eu = Identidade::nova().chaves_publicas();
        let mut agenda = Agenda::nova();
        assert_eq!(
            agenda.adicionar_por_qr(&texto_de(&eu), &eu),
            Err(ErroAgenda::Proprio)
        );
        assert!(agenda.contatos().is_empty());
    }

    #[test]
    fn qr_invalido_passa_o_motivo_e_nao_entra() {
        let eu = Identidade::nova().chaves_publicas();
        let mut agenda = Agenda::nova();
        assert_eq!(
            agenda.adicionar_por_qr("https://exemplo", &eu),
            Err(ErroAgenda::Cartao(ErroCartao::NaoEKryptos))
        );
        let mut texto = texto_de(&Identidade::nova().chaves_publicas());
        let fim = texto.len();
        let trocado = if texto.ends_with('0') { "1" } else { "0" };
        texto.replace_range(fim - 1..fim, trocado);
        assert_eq!(
            agenda.adicionar_por_qr(&texto, &eu),
            Err(ErroAgenda::Cartao(ErroCartao::NaoConfere))
        );
        assert!(agenda.contatos().is_empty());
    }

    #[test]
    fn mesma_ed25519_com_outra_curve25519_e_conflito() -> Result<(), ErroAgenda> {
        let eu = Identidade::nova().chaves_publicas();
        let outro = Identidade::nova().chaves_publicas();
        let mut agenda = Agenda::nova();
        agenda.adicionar_por_qr(&texto_de(&outro), &eu)?;
        let imitacao = IdentityKeys {
            ed25519: outro.ed25519,
            curve25519: Identidade::nova().chaves_publicas().curve25519,
        };
        assert_eq!(
            agenda.adicionar_por_qr(&texto_de(&imitacao), &eu),
            Err(ErroAgenda::Conflito)
        );
        assert_eq!(agenda.contatos().len(), 1);
        Ok(())
    }

    #[test]
    fn mesma_curve25519_com_outra_ed25519_e_conflito() -> Result<(), ErroAgenda> {
        let eu = Identidade::nova().chaves_publicas();
        let outro = Identidade::nova().chaves_publicas();
        let mut agenda = Agenda::nova();
        agenda.adicionar_por_qr(&texto_de(&outro), &eu)?;
        let imitacao = IdentityKeys {
            ed25519: Identidade::nova().chaves_publicas().ed25519,
            curve25519: outro.curve25519,
        };
        assert_eq!(
            agenda.adicionar_por_qr(&texto_de(&imitacao), &eu),
            Err(ErroAgenda::Conflito)
        );
        Ok(())
    }

    #[test]
    fn imitacao_de_uma_chave_deste_aparelho_e_conflito() {
        let eu = Identidade::nova().chaves_publicas();
        let imitacao = IdentityKeys {
            ed25519: eu.ed25519,
            curve25519: Curve25519PublicKey::from_bytes([9u8; 32]),
        };
        let mut agenda = Agenda::nova();
        assert_eq!(
            agenda.adicionar_por_qr(&texto_de(&imitacao), &eu),
            Err(ErroAgenda::Conflito)
        );
        assert!(agenda.contatos().is_empty());
    }

    /// Mesmo código de contato com chaves diferentes. Achar uma colisão de 35 bits de
    /// verdade custa horas, então o teste põe na agenda um contato com o código do
    /// cartão novo e outras chaves, montado à mão.
    #[test]
    fn mesmo_codigo_com_outras_chaves_e_conflito() {
        let eu = Identidade::nova().chaves_publicas();
        let novo = Identidade::nova();
        let mut agenda = Agenda::nova();
        agenda.contatos.push(Contato {
            cartao: CartaoContato::de_chaves(&Identidade::nova().chaves_publicas()),
            codigo: novo.codigo_contato(),
            conferencia: Conferencia::PorQr,
        });
        assert_eq!(
            agenda.adicionar_por_qr(&texto_de(&novo.chaves_publicas()), &eu),
            Err(ErroAgenda::Conflito)
        );
        assert_eq!(agenda.contatos().len(), 1);
    }

    #[test]
    fn codigos_de_erro_sao_distintos() {
        let todos = [
            ErroAgenda::Proprio.codigo(),
            ErroAgenda::Conflito.codigo(),
            ErroAgenda::SemIdentidade.codigo(),
            ErroAgenda::Cartao(ErroCartao::NaoEKryptos).codigo(),
            ErroAgenda::Cartao(ErroCartao::VersaoDesconhecida).codigo(),
            ErroAgenda::Cartao(ErroCartao::Tamanho).codigo(),
            ErroAgenda::Cartao(ErroCartao::Formato).codigo(),
            ErroAgenda::Cartao(ErroCartao::ChaveInvalida).codigo(),
            ErroAgenda::Cartao(ErroCartao::NaoConfere).codigo(),
        ];
        let mut unicos = todos.to_vec();
        unicos.sort_unstable();
        unicos.dedup();
        assert_eq!(unicos.len(), todos.len());
    }

    #[test]
    fn debug_da_agenda_nao_mostra_chave_nem_codigo() -> Result<(), ErroAgenda> {
        let eu = Identidade::nova().chaves_publicas();
        let outro = Identidade::nova();
        let mut agenda = Agenda::nova();
        agenda.adicionar_por_qr(&texto_de(&outro.chaves_publicas()), &eu)?;
        let depuracao = format!("{agenda:?}");
        let codigo = outro.codigo_contato().como_texto();
        for proibido in [
            outro.chaves_publicas().ed25519.to_base64(),
            outro.chaves_publicas().curve25519.to_base64(),
            codigo.clone(),
            codigo.replace('-', ""),
        ] {
            assert!(!depuracao.contains(&proibido), "Debug mostra {proibido}");
        }
        Ok(())
    }
}
