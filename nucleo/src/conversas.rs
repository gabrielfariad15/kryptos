//! Conversas abertas nesta execução do app (só em memória), várias por contato.
//!
//! Por que várias: os dois lados podem abrir conversa ao mesmo tempo, cada um mandando
//! a primeira antes de receber a do outro (achado no teste com os dois celulares, em
//! 07/11/2026). Com uma conversa só por contato, a primeira que chega substituía a
//! outra, e tudo o que viesse pela substituída deixava de abrir. O teste
//! `cruzamento_de_primeiras_mensagens_nao_perde_nada` (aparelho.rs) reproduz o caso.
//!
//! A regra é a do Olm no Matrix (especificação do m.olm.v1, proposta MSC1719):
//! - ao receber, tentar cada conversa guardada do remetente;
//! - ao mandar, usar a que decifrou uma mensagem por último; se nenhuma decifrou
//!   ainda, a criada por último.
//!
//! Tentar uma conversa que não é a da mensagem não estraga nada: o vodozemac só mexe
//! na conversa quando a autenticação da mensagem confere.

use vodozemac::olm::OlmMessage;
use zeroize::Zeroizing;

use crate::codigo::CodigoContato;
use crate::sessao::{ErroSessao, Sessao};

/// Conversas guardadas por contato. Estimo que 4 cubram o cruzamento e uma conversa
/// reaberta com cartas ainda no caminho; passando disso, sai a mais antiga do contato.
pub const MAXIMO_POR_CONTATO: usize = 4;

/// A ordem da lista é a de uso: a última de cada contato é a ativa (a que manda).
#[derive(Default)]
pub struct Conversas {
    lista: Vec<(CodigoContato, Sessao)>,
}

impl Conversas {
    pub fn nova() -> Self {
        Self::default()
    }

    pub fn tem(&self, codigo: CodigoContato) -> bool {
        self.lista.iter().any(|(c, _)| *c == codigo)
    }

    /// Quantas conversas guardadas com o contato.
    pub fn quantas(&self, codigo: CodigoContato) -> usize {
        self.lista.iter().filter(|(c, _)| *c == codigo).count()
    }

    /// A conversa por onde mandar para o contato.
    pub fn ativa(&mut self, codigo: CodigoContato) -> Option<&mut Sessao> {
        self.lista
            .iter_mut()
            .rev()
            .find(|(c, _)| *c == codigo)
            .map(|(_, s)| s)
    }

    /// Guarda uma conversa nova do contato, já como a ativa. As anteriores ficam, para
    /// as cartas que ainda vêm por elas.
    pub fn guardar(&mut self, codigo: CodigoContato, sessao: Sessao) {
        self.lista.push((codigo, sessao));
        while self.quantas(codigo) > MAXIMO_POR_CONTATO {
            if let Some(i) = self.lista.iter().position(|(c, _)| *c == codigo) {
                self.lista.remove(i);
            }
        }
    }

    /// Primeira mensagem (do tipo que abre conversa) de uma conversa já guardada do
    /// contato: o outro lado mandou várias antes de receber resposta. `None`: nenhuma
    /// conversa do contato tem o identificador da mensagem (é conversa nova).
    pub fn decifrar_primeira(
        &mut self,
        codigo: CodigoContato,
        mensagem: &OlmMessage,
    ) -> Option<Result<Zeroizing<Vec<u8>>, ErroSessao>> {
        let OlmMessage::PreKey(primeira) = mensagem else {
            return None;
        };
        let id = primeira.session_id();
        let i = self
            .lista
            .iter()
            .position(|(c, s)| *c == codigo && s.identificador() == id)?;
        let r = self.lista[i].1.decifrar(mensagem);
        if r.is_ok() {
            self.ativar(i);
        }
        Some(r)
    }

    /// Mensagem comum de um contato conhecido: tenta as conversas dele, da mais
    /// recente para a mais antiga. A que abrir vira a ativa. `None`: nenhuma conversa
    /// com o contato. Nenhuma abriu: o erro da última tentada.
    pub fn decifrar_comum_de(
        &mut self,
        codigo: CodigoContato,
        mensagem: &OlmMessage,
    ) -> Option<Result<Zeroizing<Vec<u8>>, ErroSessao>> {
        let mut ultimo_erro = None;
        for i in (0..self.lista.len()).rev() {
            if self.lista[i].0 != codigo {
                continue;
            }
            match self.lista[i].1.decifrar(mensagem) {
                Ok(texto) => {
                    self.ativar(i);
                    return Some(Ok(texto));
                }
                Err(e) => ultimo_erro = Some(e),
            }
        }
        ultimo_erro.map(Err)
    }

    /// Mensagem comum sem remetente conhecido (a carta não diz de quem é): tenta todas
    /// as conversas, da mais recente para a mais antiga. A que abrir vira a ativa do
    /// contato dela.
    pub fn decifrar_comum(
        &mut self,
        mensagem: &OlmMessage,
    ) -> Option<(CodigoContato, Zeroizing<Vec<u8>>)> {
        for i in (0..self.lista.len()).rev() {
            if let Ok(texto) = self.lista[i].1.decifrar(mensagem) {
                let codigo = self.lista[i].0;
                self.ativar(i);
                return Some((codigo, texto));
            }
        }
        None
    }

    fn ativar(&mut self, i: usize) {
        let item = self.lista.remove(i);
        self.lista.push(item);
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::contatos::Agenda;
    use crate::identidade::Identidade;

    /// Um lado da conversa, todo em memória: conta, o contato do outro e as conversas.
    struct Lado {
        eu: Identidade,
        outro: crate::contatos::Contato,
        conversas: Conversas,
    }

    impl Lado {
        fn codigo_do_outro(&self) -> CodigoContato {
            self.outro.codigo()
        }

        /// Abre conversa com uma chave avulsa do outro (sem ter recebido nada).
        fn abrir(&mut self, outro: &mut Lado) -> Result<(), Box<dyn std::error::Error>> {
            let avulsa = outro.eu.nova_chave_avulsa()?;
            let s = Sessao::abrir(&self.eu, &self.outro, &avulsa)?;
            self.conversas.guardar(self.codigo_do_outro(), s);
            Ok(())
        }

        fn mandar(&mut self, texto: &[u8]) -> Result<OlmMessage, Box<dyn std::error::Error>> {
            let c = self.codigo_do_outro();
            Ok(self
                .conversas
                .ativa(c)
                .ok_or("sem conversa")?
                .cifrar(texto)?)
        }

        /// O mesmo caminho do `aparelho`: primeira de conversa guardada, primeira de
        /// conversa nova ou comum.
        fn receber(&mut self, m: &OlmMessage) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
            let c = self.codigo_do_outro();
            if let Some(r) = self.conversas.decifrar_primeira(c, m) {
                return Ok(r?.to_vec());
            }
            if matches!(m, OlmMessage::PreKey(_)) {
                let (s, t) = Sessao::receber_primeira(&mut self.eu, &self.outro, m)?;
                self.conversas.guardar(c, s);
                return Ok(t.to_vec());
            }
            Ok(self
                .conversas
                .decifrar_comum_de(c, m)
                .ok_or("sem conversa")??
                .to_vec())
        }
    }

    fn par() -> Result<(Lado, Lado), Box<dyn std::error::Error>> {
        let a = Identidade::nova();
        let b = Identidade::nova();
        let mut agenda_a = Agenda::nova();
        agenda_a.adicionar_por_qr(
            &crate::cartao::CartaoContato::de_chaves(&b.chaves_publicas()).como_texto(),
            &a.chaves_publicas(),
        )?;
        let mut agenda_b = Agenda::nova();
        agenda_b.adicionar_por_qr(
            &crate::cartao::CartaoContato::de_chaves(&a.chaves_publicas()).como_texto(),
            &b.chaves_publicas(),
        )?;
        Ok((
            Lado {
                eu: a,
                outro: agenda_a.contatos()[0],
                conversas: Conversas::nova(),
            },
            Lado {
                eu: b,
                outro: agenda_b.contatos()[0],
                conversas: Conversas::nova(),
            },
        ))
    }

    #[test]
    fn cruzamento_converge_e_tudo_abre_dos_dois_lados() -> Result<(), Box<dyn std::error::Error>> {
        let (mut a, mut b) = par()?;
        a.abrir(&mut b)?;
        b.abrir(&mut a)?;
        let a1 = a.mandar(b"a1")?;
        let b1 = b.mandar(b"b1")?;
        assert_eq!(b.receber(&a1)?, b"a1");
        assert_eq!(a.receber(&b1)?, b"b1");
        assert_eq!(a.conversas.quantas(a.codigo_do_outro()), 2);

        // Daqui em diante, idas e voltas em várias ordens: tudo abre.
        for rodada in 0..4u8 {
            let x = a.mandar(&[b'a', rodada])?;
            let y = b.mandar(&[b'b', rodada])?;
            assert_eq!(b.receber(&x)?, [b'a', rodada]);
            assert_eq!(a.receber(&y)?, [b'b', rodada]);
            let z = a.mandar(&[b'c', rodada])?;
            assert_eq!(b.receber(&z)?, [b'c', rodada]);
        }
        // Os dois acabam mandando pela mesma conversa.
        let (ia, ib) = (
            a.conversas
                .ativa(a.codigo_do_outro())
                .ok_or("A")?
                .identificador(),
            b.conversas
                .ativa(b.codigo_do_outro())
                .ok_or("B")?
                .identificador(),
        );
        assert_eq!(ia, ib, "não convergiu");
        Ok(())
    }

    #[test]
    fn varias_primeiras_antes_da_resposta_abrem_na_mesma_conversa()
    -> Result<(), Box<dyn std::error::Error>> {
        let (mut a, mut b) = par()?;
        a.abrir(&mut b)?;
        let m1 = a.mandar(b"1")?;
        let m2 = a.mandar(b"2")?;
        assert!(matches!(m2, OlmMessage::PreKey(_)));
        assert_eq!(b.receber(&m1)?, b"1");
        assert_eq!(b.receber(&m2)?, b"2");
        assert_eq!(
            b.conversas.quantas(b.codigo_do_outro()),
            1,
            "abriu conversa a mais"
        );
        Ok(())
    }

    #[test]
    fn mensagem_de_conversa_antiga_ainda_abre_e_vira_a_ativa()
    -> Result<(), Box<dyn std::error::Error>> {
        let (mut a, mut b) = par()?;
        a.abrir(&mut b)?;
        assert_eq!(b.receber(&a.mandar(b"oi")?)?, b"oi");
        assert_eq!(a.receber(&b.mandar(b"oi, A")?)?, b"oi, A");
        let id_antiga = a
            .conversas
            .ativa(a.codigo_do_outro())
            .ok_or("A")?
            .identificador();
        // A abre outra conversa (perdeu a confiança na antiga, por exemplo).
        a.abrir(&mut b)?;
        let nova = a.mandar(b"nova")?;
        // B ainda manda pela antiga; A abre e passa a mandar por ela.
        let pela_antiga = b.mandar(b"pela antiga")?;
        assert_eq!(a.receber(&pela_antiga)?, b"pela antiga");
        assert_eq!(
            a.conversas
                .ativa(a.codigo_do_outro())
                .ok_or("A")?
                .identificador(),
            id_antiga
        );
        assert_eq!(b.receber(&nova)?, b"nova");
        Ok(())
    }

    #[test]
    fn maximo_por_contato_tira_a_mais_antiga() -> Result<(), Box<dyn std::error::Error>> {
        let (mut a, mut b) = par()?;
        let mut ids = Vec::new();
        for _ in 0..MAXIMO_POR_CONTATO + 2 {
            a.abrir(&mut b)?;
            ids.push(
                a.conversas
                    .ativa(a.codigo_do_outro())
                    .ok_or("A")?
                    .identificador(),
            );
        }
        assert_eq!(a.conversas.quantas(a.codigo_do_outro()), MAXIMO_POR_CONTATO);
        // a ativa é a última aberta
        assert_eq!(
            a.conversas
                .ativa(a.codigo_do_outro())
                .ok_or("A")?
                .identificador(),
            ids[ids.len() - 1]
        );
        Ok(())
    }

    #[test]
    fn mensagem_que_nao_e_de_nenhuma_conversa_nao_abre_e_nada_muda()
    -> Result<(), Box<dyn std::error::Error>> {
        let (mut a, mut b) = par()?;
        a.abrir(&mut b)?;
        assert_eq!(b.receber(&a.mandar(b"oi")?)?, b"oi");
        let resposta = b.mandar(b"r")?;
        assert_eq!(a.receber(&resposta)?, b"r");
        // Outro par: mensagem comum que ninguém daqui conhece.
        let (mut c, mut d) = par()?;
        c.abrir(&mut d)?;
        assert_eq!(d.receber(&c.mandar(b"x")?)?, b"x");
        let estranha = d.mandar(b"y")?;
        assert!(b.conversas.decifrar_comum(&estranha).is_none());
        // e a conversa de verdade continua andando
        assert_eq!(b.receber(&a.mandar(b"depois")?)?, b"depois");
        Ok(())
    }
}
