//! Sessão Olm entre duas pessoas (vodozemac, Double Ratchet), só entre contatos
//! conferidos por QR. Fica só em memória, como a agenda.
//!
//! Abrir (A escreve primeiro para B):
//!   1. B gera uma chave avulsa assinada: `Identidade::nova_chave_avulsa`.
//!   2. A chave chega a A (o caminho ainda não está decidido: QR, servidor ou reserva).
//!   3. A confere a assinatura com a Ed25519 de B lida no QR e abre: `Sessao::abrir`.
//!   4. A cifra; a primeira mensagem leva o que B precisa para abrir o lado dele.
//!   5. B abre com `Sessao::receber_primeira`, que confere que a mensagem veio da chave
//!      de A lida no QR. As seguintes, dos dois lados, vão por `cifrar` e `decifrar`.
//!
//! Recusas que este módulo garante, cada uma com teste:
//! - chave avulsa sem a assinatura do contato;
//! - chave de ordem baixa em qualquer ponto do acordo (identidade, chave avulsa, chave
//!   base, catraca): o segredo sairia previsível (`NonContributoryKey` do vodozemac);
//! - primeira mensagem que não veio da chave de quem está na agenda;
//! - mensagem adulterada, repetida ou de outra sessão.
//!
//! Olm versão 1 (MAC truncado em 8 bytes): a versão 2 do vodozemac ainda é experimental
//! e fica atrás de recurso (decisão de 07/11/2026, regra 8.1).
//!
//! Regra 8.2: os erros do vodozemac trazem chaves públicas no texto; aqui eles viram um
//! código fixo e a chave é descartada de propósito. O `Debug` esconde tudo.

use std::fmt;

use vodozemac::olm::{
    DecryptionError, EncryptionError, OlmMessage, Session, SessionConfig, SessionCreationError,
};
use vodozemac::{Curve25519PublicKey, Ed25519Signature};
use zeroize::Zeroizing;

use crate::contatos::Contato;
use crate::identidade::Identidade;

/// Separa a assinatura da chave avulsa de qualquer outra coisa que a mesma Ed25519
/// assinar no futuro. Mudar o formato exige trocar o `v1`.
const DOMINIO_CHAVE_AVULSA: &[u8] = b"KRYPTOS/chave-avulsa/v1";

/// Tamanho da assinatura Ed25519.
pub const BYTES_ASSINATURA: usize = Ed25519Signature::LENGTH;

/// Tipo da primeira mensagem (`PreKey`) e das seguintes (`Normal`) no `ler_mensagem`.
pub const TIPO_PRIMEIRA: usize = 0;
pub const TIPO_NORMAL: usize = 1;

fn configuracao() -> SessionConfig {
    SessionConfig::version_1()
}

/// Motivo de recusa. Não carrega chave nem texto (regra 8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroSessao {
    /// A chave avulsa não foi assinada pela Ed25519 do contato lida no QR.
    AssinaturaInvalida,
    /// Bytes de chave, assinatura ou mensagem fora do formato.
    Formato,
    /// O vodozemac não devolveu a chave avulsa que acabou de gerar.
    ChaveAvulsaNaoGerada,
    /// Chave de ordem baixa: o segredo combinado sairia previsível.
    ChaveFraca,
    /// A primeira mensagem não veio da chave do contato lida no QR.
    RemetenteNaoConfere,
    /// A primeira mensagem usa uma chave avulsa que este aparelho não tem: já usada,
    /// nunca gerada aqui ou perdida quando o app fechou.
    ChaveAvulsaDesconhecida,
    /// A mensagem foi feita com outra versão do Olm.
    VersaoDoOlm,
    /// Esperava a primeira mensagem de uma sessão e veio uma mensagem comum.
    NaoEPrimeira,
    /// O MAC não confere: mensagem adulterada ou de outra sessão.
    Adulterada,
    /// A chave desta mensagem já foi usada (mensagem repetida) ou descartada.
    JaUsada,
    /// Mensagens demais puladas na mesma cadeia.
    SaltoGrande,
}

impl ErroSessao {
    /// Código fixo que atravessa a ponte e vira texto na tela.
    pub fn codigo(&self) -> &'static str {
        match self {
            Self::AssinaturaInvalida => "SESSAO_ASSINATURA_INVALIDA",
            Self::Formato => "SESSAO_FORMATO",
            Self::ChaveAvulsaNaoGerada => "SESSAO_CHAVE_AVULSA_NAO_GERADA",
            Self::ChaveFraca => "SESSAO_CHAVE_FRACA",
            Self::RemetenteNaoConfere => "SESSAO_REMETENTE_NAO_CONFERE",
            Self::ChaveAvulsaDesconhecida => "SESSAO_CHAVE_AVULSA_DESCONHECIDA",
            Self::VersaoDoOlm => "SESSAO_VERSAO_DO_OLM",
            Self::NaoEPrimeira => "SESSAO_NAO_E_PRIMEIRA",
            Self::Adulterada => "SESSAO_ADULTERADA",
            Self::JaUsada => "SESSAO_JA_USADA",
            Self::SaltoGrande => "SESSAO_SALTO_GRANDE",
        }
    }

    // As conversões abaixo descartam a chave que o erro do vodozemac carrega (regra
    // 8.2). O motivo não se perde: cada variante tem o seu código. Sem `_ =>`: uma
    // variante nova no vodozemac quebra a compilação em vez de cair num código errado.

    fn de_criacao(e: SessionCreationError) -> Self {
        match e {
            SessionCreationError::MissingOneTimeKey(_) => Self::ChaveAvulsaDesconhecida,
            SessionCreationError::MismatchedIdentityKey(_, _) => Self::RemetenteNaoConfere,
            SessionCreationError::MismatchedSessionConfig { .. } => Self::VersaoDoOlm,
            SessionCreationError::Decryption(d) => Self::de_decifracao(d),
            SessionCreationError::NonContributoryKey => Self::ChaveFraca,
        }
    }

    fn de_decifracao(e: DecryptionError) -> Self {
        match e {
            DecryptionError::InvalidMAC(_)
            | DecryptionError::InvalidMACLength(_, _)
            | DecryptionError::InvalidPadding(_) => Self::Adulterada,
            DecryptionError::NonContributoryKey => Self::ChaveFraca,
            DecryptionError::MissingMessageKey(_) => Self::JaUsada,
            DecryptionError::TooBigMessageGap(_, _) => Self::SaltoGrande,
        }
    }

    fn de_cifracao(e: EncryptionError) -> Self {
        match e {
            EncryptionError::NonContributoryKey => Self::ChaveFraca,
        }
    }
}

impl fmt::Display for ErroSessao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErroSessao {}

/// Chave Curve25519 de uso único de B, assinada com a Ed25519 de B. É o que A precisa,
/// além do QR, para abrir a sessão com B sem B estar presente.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ChaveAvulsa {
    chave: Curve25519PublicKey,
    assinatura: Ed25519Signature,
}

impl ChaveAvulsa {
    /// Monta a partir dos bytes que chegaram de fora. Não confere a assinatura: isso
    /// é feito em `Sessao::abrir`, contra o contato.
    pub fn de_partes(chave: [u8; 32], assinatura: &[u8]) -> Result<Self, ErroSessao> {
        if assinatura.len() != BYTES_ASSINATURA {
            return Err(ErroSessao::Formato);
        }
        let assinatura =
            Ed25519Signature::from_slice(assinatura).map_err(|_| ErroSessao::Formato)?;
        Ok(Self {
            chave: Curve25519PublicKey::from_bytes(chave),
            assinatura,
        })
    }

    pub fn chave_bytes(&self) -> [u8; 32] {
        self.chave.to_bytes()
    }

    pub fn assinatura_bytes(&self) -> [u8; BYTES_ASSINATURA] {
        self.assinatura.to_bytes()
    }

    fn assinado(chave: &Curve25519PublicKey) -> Vec<u8> {
        let mut m = Vec::with_capacity(DOMINIO_CHAVE_AVULSA.len() + 32);
        m.extend_from_slice(DOMINIO_CHAVE_AVULSA);
        m.extend_from_slice(chave.as_bytes());
        m
    }

    fn conferir(&self, contato: &Contato) -> Result<(), ErroSessao> {
        contato
            .chaves()
            .ed25519
            .verify(&Self::assinado(&self.chave), &self.assinatura)
            .map_err(|_| ErroSessao::AssinaturaInvalida)
    }
}

impl fmt::Debug for ChaveAvulsa {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChaveAvulsa(oculta)")
    }
}

impl Identidade {
    /// Gera uma chave avulsa nova, assinada por esta identidade, e a marca como
    /// publicada. A parte secreta fica na conta, em memória, até a primeira mensagem
    /// que a usar. A conta guarda no máximo 50; a 51ª descarta a mais antiga.
    pub fn nova_chave_avulsa(&mut self) -> Result<ChaveAvulsa, ErroSessao> {
        let conta = self.conta_mut();
        let gerada = conta.generate_one_time_keys(1);
        let chave = gerada
            .created
            .first()
            .copied()
            .ok_or(ErroSessao::ChaveAvulsaNaoGerada)?;
        let assinatura = conta.sign(ChaveAvulsa::assinado(&chave));
        conta.mark_keys_as_published();
        Ok(ChaveAvulsa { chave, assinatura })
    }
}

/// Lê uma mensagem que chegou de fora: tipo (`TIPO_PRIMEIRA` ou `TIPO_NORMAL`) e bytes.
pub fn ler_mensagem(tipo: usize, bytes: &[u8]) -> Result<OlmMessage, ErroSessao> {
    OlmMessage::from_parts(tipo, bytes).map_err(|_| ErroSessao::Formato)
}

/// Uma ponta da conversa com um contato.
pub struct Sessao {
    interna: Session,
}

impl Sessao {
    /// A abre a sessão com B: confere a chave avulsa contra a Ed25519 de B lida no QR
    /// e faz o acordo de chaves. A primeira mensagem que A cifrar leva o resto.
    pub fn abrir(
        eu: &Identidade,
        contato: &Contato,
        avulsa: &ChaveAvulsa,
    ) -> Result<Self, ErroSessao> {
        avulsa.conferir(contato)?;
        let interna = eu
            .conta()
            .create_outbound_session(configuracao(), contato.chaves().curve25519, avulsa.chave)
            .map_err(ErroSessao::de_criacao)?;
        Ok(Self { interna })
    }

    /// B abre o lado dele com a primeira mensagem de A e já devolve o texto dela. Só
    /// aceita se a mensagem veio da Curve25519 de A lida no QR. Mensagem recusada não
    /// gasta a chave avulsa.
    pub fn receber_primeira(
        eu: &mut Identidade,
        contato: &Contato,
        mensagem: &OlmMessage,
    ) -> Result<(Self, Zeroizing<Vec<u8>>), ErroSessao> {
        let OlmMessage::PreKey(primeira) = mensagem else {
            return Err(ErroSessao::NaoEPrimeira);
        };
        let aberta = eu
            .conta_mut()
            .create_inbound_session(configuracao(), contato.chaves().curve25519, primeira)
            .map_err(ErroSessao::de_criacao)?;
        Ok((
            Self {
                interna: aberta.session,
            },
            Zeroizing::new(aberta.plaintext),
        ))
    }

    pub fn cifrar(&mut self, texto: &[u8]) -> Result<OlmMessage, ErroSessao> {
        self.interna.encrypt(texto).map_err(ErroSessao::de_cifracao)
    }

    /// Decifra uma mensagem desta sessão. Também aceita as primeiras mensagens que A
    /// mandou antes de B responder.
    pub fn decifrar(&mut self, mensagem: &OlmMessage) -> Result<Zeroizing<Vec<u8>>, ErroSessao> {
        self.interna
            .decrypt(mensagem)
            .map(Zeroizing::new)
            .map_err(ErroSessao::de_decifracao)
    }

    /// Identificador da sessão, igual nas duas pontas. Não é segredo.
    pub fn identificador(&self) -> String {
        self.interna.session_id()
    }
}

impl fmt::Debug for Sessao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sessao(oculta)")
    }
}

#[cfg(test)]
mod testes {
    use std::error::Error;

    use vodozemac::olm::IdentityKeys;
    use vodozemac::{Curve25519PublicKey, Ed25519PublicKey};

    use super::*;
    use crate::cartao::CartaoContato;
    use crate::contatos::Agenda;

    type R = Result<(), Box<dyn Error>>;

    fn de_hex(texto: &str) -> Result<Vec<u8>, Box<dyn Error>> {
        if !texto.len().is_multiple_of(2) {
            return Err("hex de tamanho ímpar".into());
        }
        (0..texto.len())
            .step_by(2)
            .map(|i| Ok(u8::from_str_radix(&texto[i..i + 2], 16)?))
            .collect()
    }

    fn de_hex_32(texto: &str) -> Result<[u8; 32], Box<dyn Error>> {
        de_hex(texto)?
            .try_into()
            .map_err(|_| "chave sem 32 bytes".into())
    }

    /// `quem` visto por `eu`: o contato entra pela agenda, lido do QR, como no app.
    fn contato(eu: &IdentityKeys, quem: &IdentityKeys) -> Result<Contato, Box<dyn Error>> {
        let mut agenda = Agenda::nova();
        agenda.adicionar_por_qr(&CartaoContato::de_chaves(quem).como_texto(), eu)?;
        Ok(agenda.contatos()[0])
    }

    struct Par {
        a: Identidade,
        b: Identidade,
        /// B na agenda de A.
        b_para_a: Contato,
        /// A na agenda de B.
        a_para_b: Contato,
    }

    fn par() -> Result<Par, Box<dyn Error>> {
        let a = Identidade::nova();
        let b = Identidade::nova();
        let b_para_a = contato(&a.chaves_publicas(), &b.chaves_publicas())?;
        let a_para_b = contato(&b.chaves_publicas(), &a.chaves_publicas())?;
        Ok(Par {
            a,
            b,
            b_para_a,
            a_para_b,
        })
    }

    /// A abre, manda `texto`, B recebe. Devolve as duas pontas.
    fn conversa(p: &mut Par, texto: &[u8]) -> Result<(Sessao, Sessao), Box<dyn Error>> {
        let avulsa = p.b.nova_chave_avulsa()?;
        let mut sa = Sessao::abrir(&p.a, &p.b_para_a, &avulsa)?;
        let primeira = sa.cifrar(texto)?;
        let (sb, lido) = Sessao::receber_primeira(&mut p.b, &p.a_para_b, &primeira)?;
        assert_eq!(lido.as_slice(), texto);
        Ok((sa, sb))
    }

    /// Assina qualquer chave com a Ed25519 de `quem`: simula um contato que assina uma
    /// chave ruim de propósito, para a recusa vir da chave e não da assinatura.
    fn assinada_por(quem: &Identidade, chave: Curve25519PublicKey) -> ChaveAvulsa {
        ChaveAvulsa {
            chave,
            assinatura: quem.conta().sign(ChaveAvulsa::assinado(&chave)),
        }
    }

    fn partes(m: &OlmMessage) -> (usize, Vec<u8>) {
        m.to_parts()
    }

    // ---------- pronto quando 1: A cifra, B decifra ----------

    #[test]
    fn a_cifra_e_b_decifra_nos_dois_sentidos() -> R {
        let mut p = par()?;
        let (mut sa, mut sb) = conversa(&mut p, b"oi, B")?;
        assert_eq!(sa.identificador(), sb.identificador());
        // 20 idas e voltas: cada troca de sentido gira a catraca de Diffie-Hellman.
        for i in 0..20u32 {
            let ida = format!("A para B, {i}");
            let m = sa.cifrar(ida.as_bytes())?;
            assert_eq!(sb.decifrar(&m)?.as_slice(), ida.as_bytes());
            let volta = format!("B para A, {i}");
            let m = sb.cifrar(volta.as_bytes())?;
            assert_eq!(partes(&m).0, TIPO_NORMAL);
            assert_eq!(sa.decifrar(&m)?.as_slice(), volta.as_bytes());
        }
        Ok(())
    }

    #[test]
    fn texto_vazio_e_texto_grande_vao_e_voltam() -> R {
        let mut p = par()?;
        let (mut sa, mut sb) = conversa(&mut p, b"")?;
        let grande: Vec<u8> = (0..65_536u32).map(|i| (i % 251) as u8).collect();
        let m = sa.cifrar(&grande)?;
        assert_eq!(sb.decifrar(&m)?.as_slice(), grande.as_slice());
        let m = sb.cifrar(b"")?;
        assert!(sa.decifrar(&m)?.is_empty());
        Ok(())
    }

    #[test]
    fn varias_primeiras_antes_da_resposta_abrem_na_mesma_sessao() -> R {
        let mut p = par()?;
        let avulsa = p.b.nova_chave_avulsa()?;
        let mut sa = Sessao::abrir(&p.a, &p.b_para_a, &avulsa)?;
        let m1 = sa.cifrar(b"um")?;
        let m2 = sa.cifrar(b"dois")?;
        assert_eq!(partes(&m2).0, TIPO_PRIMEIRA);
        let (mut sb, lido) = Sessao::receber_primeira(&mut p.b, &p.a_para_b, &m1)?;
        assert_eq!(lido.as_slice(), b"um");
        // A segunda vai pela sessão já aberta; abrir outra com ela gastaria a chave
        // avulsa que já foi gasta.
        assert_eq!(
            Sessao::receber_primeira(&mut p.b, &p.a_para_b, &m2).err(),
            Some(ErroSessao::ChaveAvulsaDesconhecida)
        );
        assert_eq!(sb.decifrar(&m2)?.as_slice(), b"dois");
        Ok(())
    }

    #[test]
    fn mensagens_fora_de_ordem_abrem() -> R {
        let mut p = par()?;
        let (mut sa, mut sb) = conversa(&mut p, b"oi")?;
        let resposta = sb.cifrar(b"oi, A")?;
        sa.decifrar(&resposta)?;
        let m: Vec<OlmMessage> = (0..3)
            .map(|i| sa.cifrar(format!("n{i}").as_bytes()))
            .collect::<Result<_, _>>()?;
        for i in [2usize, 0, 1] {
            assert_eq!(sb.decifrar(&m[i])?.as_slice(), format!("n{i}").as_bytes());
        }
        Ok(())
    }

    #[test]
    fn mensagem_que_chega_pelo_formato_de_fora_abre() -> R {
        let mut p = par()?;
        let avulsa = p.b.nova_chave_avulsa()?;
        // A chave avulsa também viaja em bytes.
        let avulsa = ChaveAvulsa::de_partes(avulsa.chave_bytes(), &avulsa.assinatura_bytes())?;
        let mut sa = Sessao::abrir(&p.a, &p.b_para_a, &avulsa)?;
        let (tipo, bytes) = partes(&sa.cifrar(b"pela rede")?);
        let chegou = ler_mensagem(tipo, &bytes)?;
        let (_, lido) = Sessao::receber_primeira(&mut p.b, &p.a_para_b, &chegou)?;
        assert_eq!(lido.as_slice(), b"pela rede");
        assert_eq!(ler_mensagem(7, &bytes).err(), Some(ErroSessao::Formato));
        assert_eq!(
            ler_mensagem(TIPO_PRIMEIRA, &[]).err(),
            Some(ErroSessao::Formato)
        );
        assert_eq!(
            ChaveAvulsa::de_partes([1; 32], &[0; 63]).err(),
            Some(ErroSessao::Formato)
        );
        Ok(())
    }

    // ---------- vetor conhecido (regra 8.4): ferramentas/oraculos/olm.py ----------

    /// Conta fixa de B: a mesma do cofre (Ed25519 do teste 1 da RFC 8032, Curve25519
    /// da Alice da RFC 7748), com a chave avulsa secreta 0x50..0x6f do oráculo.
    const B_FIXO_JSON: &str = r#"{"signing_key":{"Normal":[157,97,177,157,239,253,90,96,186,132,74,244,146,236,44,196,68,73,197,105,123,50,105,25,112,59,172,3,28,174,127,96]},"diffie_hellman_key":[119,7,109,10,115,24,165,125,60,22,193,114,81,178,102,69,223,76,47,135,235,192,153,42,177,119,251,165,29,185,44,42],"one_time_keys":{"next_key_id":1,"public_keys":{},"private_keys":{"0":[80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111]}},"fallback_keys":{"key_id":0,"fallback_key":null,"previous_fallback_key":null}}"#;

    /// A no QR: Ed25519 pública do teste 2 da RFC 8032 e Curve25519 pública do Bob da
    /// RFC 7748 (a secreta é a que o oráculo usa).
    const A_ED25519: &str = "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c";
    const A_CURVE25519: &str = "de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f";
    const B_AVULSA_PUBLICA: &str =
        "392d174a38b3b1beafaf1fe824870841c5fa531bc6eafdb6402c124664488c1c";
    const VETOR_PRIMEIRA: &str = "030a20392d174a38b3b1beafaf1fe824870841c5fa531bc6eafdb6402c124664488c1c122023b7bb8c91ae008711fb12846780bcdf1e065f821bdfec49f57e7c7dcd4c48231a20de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f224f030a209fd7ad6dcff4298dd3f96d5b1b2af910a0535b1488d7f8fabb349a982880b61510002220b2b4b0e88344d85c049b4dc341723e7609ff2148839f84a0337c073a2caab4654dda2f8940c6a5aa";
    const VETOR_TEXTO: &[u8] = b"KRYPTOS vetor 1: A para B";

    fn vetor() -> Result<(Identidade, Contato, Vec<u8>), Box<dyn Error>> {
        let b = Identidade::de_pickle_json(B_FIXO_JSON)?;
        let a = IdentityKeys {
            ed25519: Ed25519PublicKey::from_slice(&de_hex_32(A_ED25519)?)?,
            curve25519: Curve25519PublicKey::from_bytes(de_hex_32(A_CURVE25519)?),
        };
        let a_para_b = contato(&b.chaves_publicas(), &a)?;
        Ok((b, a_para_b, de_hex(VETOR_PRIMEIRA)?))
    }

    #[test]
    fn vetor_conhecido_do_oraculo_abre() -> R {
        let (mut b, a_para_b, bytes) = vetor()?;
        // Calibração da conta fixa: é a mesma do vetor do código de contato, e a chave
        // avulsa que a mensagem do oráculo usa é a pública da secreta do JSON.
        assert_eq!(b.codigo_contato().como_texto(), "2J2F-P63Y");
        assert_eq!(bytes.get(3..35), Some(&de_hex_32(B_AVULSA_PUBLICA)?[..]));
        let m = ler_mensagem(TIPO_PRIMEIRA, &bytes)?;
        let (_, lido) = Sessao::receber_primeira(&mut b, &a_para_b, &m)?;
        assert_eq!(lido.as_slice(), VETOR_TEXTO);
        // A chave avulsa foi gasta: a mesma mensagem não abre outra sessão.
        assert_eq!(
            Sessao::receber_primeira(&mut b, &a_para_b, &m).err(),
            Some(ErroSessao::ChaveAvulsaDesconhecida)
        );
        Ok(())
    }

    /// Resultado de uma mensagem adulterada: o motivo da recusa ou, se a leitura der a
    /// mesma mensagem, `GRAFIA_EQUIVALENTE`. O Olm confere o MAC sobre a mensagem
    /// recodificada, e o X25519 ignora o bit 255 da chave; por isso alguns bits trocados
    /// dão bytes diferentes com a mesma mensagem dentro (mesma chave, mesmo texto
    /// cifrado, mesmo MAC). Isso não muda o texto que chega; a mensagem equivalente não
    /// é decifrada no teste, porque gastaria a chave da original.
    const EQUIVALENTE: &str = "GRAFIA_EQUIVALENTE";

    /// Adulteração (regra 8.4): cada bit de cada byte do vetor, trocado um por vez, é
    /// recusado ou dá a mesma mensagem, e nenhuma recusa gasta a chave avulsa: o vetor
    /// certo abre no fim. Contagens medidas em 07/11/2026 com o vodozemac 0.11.2.
    #[test]
    fn vetor_com_qualquer_bit_trocado_e_recusado() -> R {
        let (mut b, a_para_b, original) = vetor()?;
        let lida = ler_mensagem(TIPO_PRIMEIRA, &original)?;
        let mut por_motivo: std::collections::BTreeMap<&'static str, usize> = Default::default();
        for posicao in 0..original.len() {
            for bit in 0..8 {
                let mut adulterado = original.clone();
                adulterado[posicao] ^= 1 << bit;
                let motivo = match ler_mensagem(TIPO_PRIMEIRA, &adulterado) {
                    Err(e) => e.codigo(),
                    Ok(m) if m == lida => EQUIVALENTE,
                    Ok(m) => match Sessao::receber_primeira(&mut b, &a_para_b, &m) {
                        Ok(_) => return Err(format!("byte {posicao}, bit {bit} passou").into()),
                        Err(e) => e.codigo(),
                    },
                };
                *por_motivo.entry(motivo).or_default() += 1;
            }
        }
        let medido: Vec<(&str, usize)> = por_motivo.into_iter().collect();
        assert_eq!(
            medido.iter().map(|(_, n)| n).sum::<usize>(),
            original.len() * 8
        );
        assert_eq!(medido, MOTIVOS_DO_VETOR_ADULTERADO, "recusas por motivo");
        let (_, lido) = Sessao::receber_primeira(&mut b, &a_para_b, &lida)?;
        assert_eq!(lido.as_slice(), VETOR_TEXTO);
        Ok(())
    }

    /// 184 bytes × 8 bits. Cada chave de 32 bytes do vetor tem 255 bits que mudam a
    /// chave e 1 (o bit 255) que não muda: avulsa trocada é desconhecida, identidade
    /// trocada não confere com a agenda, chave base trocada derruba o MAC.
    const MOTIVOS_DO_VETOR_ADULTERADO: &[(&str, usize)] = &[
        (EQUIVALENTE, 7),
        ("SESSAO_ADULTERADA", 840),
        ("SESSAO_CHAVE_AVULSA_DESCONHECIDA", 255),
        ("SESSAO_FORMATO", 115),
        ("SESSAO_REMETENTE_NAO_CONFERE", 255),
    ];

    // ---------- pronto quando 2: mensagem adulterada é recusada ----------

    /// Mensagem comum no meio da conversa, cada bit trocado: recusada ou a mesma
    /// mensagem, sem estragar a sessão (a original abre depois de todas as tentativas).
    /// Repetido com 5 conversas novas. Medido em 300 conversas (07/11/2026): sempre 4
    /// grafias equivalentes; o motivo das outras recusas varia com os bytes sorteados
    /// (um comprimento trocado pode cair em formato ou em salto grande), então só o
    /// total e as equivalentes são conferidos.
    #[test]
    fn mensagem_comum_com_qualquer_bit_trocado_e_recusada() -> R {
        for _ in 0..5 {
            let mut p = par()?;
            let (mut sa, mut sb) = conversa(&mut p, b"oi")?;
            sa.decifrar(&sb.cifrar(b"oi, A")?)?;
            let certa = sa.cifrar(b"segredo")?;
            let (tipo, original) = partes(&certa);
            assert_eq!(tipo, TIPO_NORMAL);
            let mut por_motivo: std::collections::BTreeMap<&'static str, usize> =
                Default::default();
            for posicao in 0..original.len() {
                for bit in 0..8 {
                    let mut adulterado = original.clone();
                    adulterado[posicao] ^= 1 << bit;
                    let motivo = match ler_mensagem(tipo, &adulterado) {
                        Err(e) => e.codigo(),
                        Ok(m) if m == certa => EQUIVALENTE,
                        Ok(m) => match sb.decifrar(&m) {
                            Ok(_) => {
                                return Err(format!("byte {posicao}, bit {bit} passou").into());
                            }
                            Err(e) => e.codigo(),
                        },
                    };
                    *por_motivo.entry(motivo).or_default() += 1;
                }
            }
            assert_eq!(por_motivo.values().sum::<usize>(), original.len() * 8);
            assert_eq!(por_motivo.get(EQUIVALENTE), Some(&4), "{por_motivo:?}");
            assert_eq!(sb.decifrar(&certa)?.as_slice(), b"segredo");
        }
        Ok(())
    }

    #[test]
    fn mensagem_repetida_e_recusada() -> R {
        let mut p = par()?;
        let (mut sa, mut sb) = conversa(&mut p, b"oi")?;
        let m = sa.cifrar(b"uma vez")?;
        sb.decifrar(&m)?;
        assert_eq!(sb.decifrar(&m).err(), Some(ErroSessao::JaUsada));
        Ok(())
    }

    #[test]
    fn mensagem_de_outra_sessao_e_recusada() -> R {
        let mut p = par()?;
        let (mut sa, _) = conversa(&mut p, b"oi")?;
        let (_, mut outra_b) = conversa(&mut p, b"outra")?;
        let m = sa.cifrar(b"para a primeira")?;
        assert!(outra_b.decifrar(&m).is_err());
        Ok(())
    }

    #[test]
    fn mensagem_comum_nao_abre_sessao() -> R {
        let mut p = par()?;
        let (mut sa, mut sb) = conversa(&mut p, b"oi")?;
        sa.decifrar(&sb.cifrar(b"oi, A")?)?;
        let comum = sa.cifrar(b"x")?;
        assert_eq!(
            Sessao::receber_primeira(&mut p.b, &p.a_para_b, &comum).err(),
            Some(ErroSessao::NaoEPrimeira)
        );
        Ok(())
    }

    // ---------- quem é quem: assinatura e remetente ----------

    #[test]
    fn chave_avulsa_assinada_por_outro_e_recusada() -> R {
        let mut p = par()?;
        let mut c = Identidade::nova();
        // C assina a própria chave e diz que é de B.
        let de_c = c.nova_chave_avulsa()?;
        assert_eq!(
            Sessao::abrir(&p.a, &p.b_para_a, &de_c).err(),
            Some(ErroSessao::AssinaturaInvalida)
        );
        // A assinatura de B com outra chave no lugar.
        let de_b = p.b.nova_chave_avulsa()?;
        let trocada = ChaveAvulsa {
            chave: de_c.chave,
            assinatura: de_b.assinatura,
        };
        assert_eq!(
            Sessao::abrir(&p.a, &p.b_para_a, &trocada).err(),
            Some(ErroSessao::AssinaturaInvalida)
        );
        // Assinatura de B sobre a chave sem o domínio do Kryptos.
        let sem_dominio = ChaveAvulsa {
            chave: de_b.chave,
            assinatura: p.b.conta().sign(de_b.chave.as_bytes()),
        };
        assert_eq!(
            Sessao::abrir(&p.a, &p.b_para_a, &sem_dominio).err(),
            Some(ErroSessao::AssinaturaInvalida)
        );
        // Calibração: a original passa.
        Sessao::abrir(&p.a, &p.b_para_a, &de_b)?;
        Ok(())
    }

    /// C usa a chave avulsa de B e se passa por A. B recusa, e a chave avulsa continua
    /// valendo para o A de verdade.
    #[test]
    fn primeira_mensagem_de_quem_nao_e_o_contato_e_recusada() -> R {
        let mut p = par()?;
        let c = Identidade::nova();
        let b_para_c = contato(&c.chaves_publicas(), &p.b.chaves_publicas())?;
        let avulsa = p.b.nova_chave_avulsa()?;
        let de_c = Sessao::abrir(&c, &b_para_c, &avulsa)?.cifrar(b"sou o A")?;
        assert_eq!(
            Sessao::receber_primeira(&mut p.b, &p.a_para_b, &de_c).err(),
            Some(ErroSessao::RemetenteNaoConfere)
        );
        let de_a = Sessao::abrir(&p.a, &p.b_para_a, &avulsa)?.cifrar(b"sou eu")?;
        let (_, lido) = Sessao::receber_primeira(&mut p.b, &p.a_para_b, &de_a)?;
        assert_eq!(lido.as_slice(), b"sou eu");
        Ok(())
    }

    #[test]
    fn primeira_mensagem_para_outro_aparelho_e_recusada() -> R {
        let mut p = par()?;
        let mut c = Identidade::nova();
        let a_para_c = contato(&c.chaves_publicas(), &p.a.chaves_publicas())?;
        let avulsa = p.b.nova_chave_avulsa()?;
        let para_b = Sessao::abrir(&p.a, &p.b_para_a, &avulsa)?.cifrar(b"para B")?;
        assert_eq!(
            Sessao::receber_primeira(&mut c, &a_para_c, &para_b).err(),
            Some(ErroSessao::ChaveAvulsaDesconhecida)
        );
        Ok(())
    }

    // ---------- pronto quando 3: chave de ordem baixa ----------

    /// As 14 grafias de 32 bytes que zeram o X25519, calculadas e conferidas fora do
    /// Rust por ferramentas/oraculos/ordem_baixa.py.
    const ORDEM_BAIXA: [&str; 14] = [
        "0000000000000000000000000000000000000000000000000000000000000000",
        "0000000000000000000000000000000000000000000000000000000000000080",
        "edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
        "edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "0100000000000000000000000000000000000000000000000000000000000000",
        "0100000000000000000000000000000000000000000000000000000000000080",
        "eeffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
        "eeffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "e0eb7a7c3b41b8ae1656e3faf19fc46ada098deb9c32b1fd866205165f49b800",
        "e0eb7a7c3b41b8ae1656e3faf19fc46ada098deb9c32b1fd866205165f49b880",
        "5f9c95bca3508c24b1d0b1559c83ef5b04445cc4581c8e86d8224eddd09f1157",
        "5f9c95bca3508c24b1d0b1559c83ef5b04445cc4581c8e86d8224eddd09f11d7",
        "ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
        "ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    ];

    fn ordem_baixa() -> Result<Vec<Curve25519PublicKey>, Box<dyn Error>> {
        let mut grafias = ORDEM_BAIXA.to_vec();
        grafias.sort_unstable();
        grafias.dedup();
        assert_eq!(grafias.len(), ORDEM_BAIXA.len(), "lista com repetição");
        ORDEM_BAIXA
            .iter()
            .map(|h| Ok(Curve25519PublicKey::from_bytes(de_hex_32(h)?)))
            .collect()
    }

    /// Como identidade do contato (lida no QR) e como chave avulsa assinada pelo
    /// próprio contato: A recusa abrir nos dois casos.
    #[test]
    fn chave_de_ordem_baixa_e_recusada_ao_abrir() -> R {
        let mut p = par()?;
        let avulsa_boa = p.b.nova_chave_avulsa()?;
        for fraca in ordem_baixa()? {
            let imitacao = IdentityKeys {
                ed25519: p.b.chaves_publicas().ed25519,
                curve25519: fraca,
            };
            let b_fraco = contato(&p.a.chaves_publicas(), &imitacao)?;
            assert_eq!(
                Sessao::abrir(&p.a, &b_fraco, &avulsa_boa).err(),
                Some(ErroSessao::ChaveFraca),
                "identidade de ordem baixa"
            );
            assert_eq!(
                Sessao::abrir(&p.a, &p.b_para_a, &assinada_por(&p.b, fraca)).err(),
                Some(ErroSessao::ChaveFraca),
                "chave avulsa de ordem baixa"
            );
        }
        // Calibração: o mesmo caminho com chaves comuns abre.
        let comum = Identidade::nova().chaves_publicas().curve25519;
        Sessao::abrir(&p.a, &p.b_para_a, &assinada_por(&p.b, comum))?;
        Sessao::abrir(&p.a, &p.b_para_a, &avulsa_boa)?;
        Ok(())
    }

    /// Posição de cada chave na primeira mensagem (protobuf do Olm): campo 1 chave
    /// avulsa, 2 chave base, 3 identidade de quem manda, cada um com 32 bytes.
    fn trocar_campo(
        primeira: &[u8],
        campo: u8,
        chave: &[u8; 32],
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let marca = [(campo << 3) | 2, 32];
        let inicio = match campo {
            1 => 1,
            2 => 35,
            3 => 69,
            _ => return Err("campo sem chave".into()),
        };
        if primeira.get(inicio..inicio + 2) != Some(&marca[..]) {
            return Err(format!("campo {campo} fora do lugar esperado").into());
        }
        let mut saida = primeira.to_vec();
        saida[inicio + 2..inicio + 34].copy_from_slice(chave);
        Ok(saida)
    }

    /// Na primeira mensagem, como identidade de quem manda (e na agenda de B) e como
    /// chave base: B recusa, e a chave avulsa continua valendo.
    #[test]
    fn chave_de_ordem_baixa_e_recusada_ao_receber() -> R {
        let mut p = par()?;
        let avulsa = p.b.nova_chave_avulsa()?;
        let (tipo, original) = partes(&Sessao::abrir(&p.a, &p.b_para_a, &avulsa)?.cifrar(b"oi")?);
        assert_eq!(tipo, TIPO_PRIMEIRA);
        for fraca in ordem_baixa()? {
            let bytes = fraca.to_bytes();
            // Identidade fraca: a agenda de B tem de ter a mesma, senão a recusa seria
            // por remetente e não pela chave.
            let imitacao = IdentityKeys {
                ed25519: p.a.chaves_publicas().ed25519,
                curve25519: fraca,
            };
            let a_fraco = contato(&p.b.chaves_publicas(), &imitacao)?;
            let m = ler_mensagem(tipo, &trocar_campo(&original, 3, &bytes)?)?;
            assert_eq!(
                Sessao::receber_primeira(&mut p.b, &a_fraco, &m).err(),
                Some(ErroSessao::ChaveFraca),
                "identidade de ordem baixa"
            );
            let m = ler_mensagem(tipo, &trocar_campo(&original, 2, &bytes)?)?;
            assert_eq!(
                Sessao::receber_primeira(&mut p.b, &p.a_para_b, &m).err(),
                Some(ErroSessao::ChaveFraca),
                "chave base de ordem baixa"
            );
        }
        // Calibração: a mensagem original abre, e por isso nenhuma recusa gastou a chave.
        let m = ler_mensagem(tipo, &original)?;
        let (_, lido) = Sessao::receber_primeira(&mut p.b, &p.a_para_b, &m)?;
        assert_eq!(lido.as_slice(), b"oi");
        Ok(())
    }

    /// Catraca de ordem baixa numa mensagem comum (campo 1, logo depois da versão):
    /// recusada, e a sessão continua aceitando a mensagem certa.
    #[test]
    fn catraca_de_ordem_baixa_e_recusada() -> R {
        let mut p = par()?;
        let (mut sa, mut sb) = conversa(&mut p, b"oi")?;
        sa.decifrar(&sb.cifrar(b"oi, A")?)?;
        let (tipo, original) = partes(&sa.cifrar(b"depois")?);
        assert_eq!(
            original.get(1..3),
            Some(&[0x0a, 32][..]),
            "catraca fora do lugar"
        );
        for fraca in ordem_baixa()? {
            let mut bytes = original.clone();
            bytes[3..35].copy_from_slice(&fraca.to_bytes());
            let m = ler_mensagem(tipo, &bytes)?;
            assert_eq!(sb.decifrar(&m).err(), Some(ErroSessao::ChaveFraca));
        }
        assert_eq!(
            sb.decifrar(&ler_mensagem(tipo, &original)?)?.as_slice(),
            b"depois"
        );
        Ok(())
    }

    // ---------- regra 8.2 e códigos ----------

    #[test]
    fn debug_nao_mostra_chave() -> R {
        let mut p = par()?;
        let avulsa = p.b.nova_chave_avulsa()?;
        let sessao = Sessao::abrir(&p.a, &p.b_para_a, &avulsa)?;
        let depuracao = format!("{avulsa:?} {sessao:?} {:?}", ErroSessao::ChaveFraca);
        for proibido in [
            avulsa.chave.to_base64(),
            avulsa.assinatura.to_base64(),
            p.b.chaves_publicas().curve25519.to_base64(),
            p.a.chaves_publicas().curve25519.to_base64(),
        ] {
            assert!(!depuracao.contains(&proibido), "Debug mostra {proibido}");
        }
        Ok(())
    }

    #[test]
    fn codigos_de_erro_sao_distintos() {
        let todos = [
            ErroSessao::AssinaturaInvalida,
            ErroSessao::Formato,
            ErroSessao::ChaveAvulsaNaoGerada,
            ErroSessao::ChaveFraca,
            ErroSessao::RemetenteNaoConfere,
            ErroSessao::ChaveAvulsaDesconhecida,
            ErroSessao::VersaoDoOlm,
            ErroSessao::NaoEPrimeira,
            ErroSessao::Adulterada,
            ErroSessao::JaUsada,
            ErroSessao::SaltoGrande,
        ]
        .map(|e| e.codigo());
        let mut unicos = todos.to_vec();
        unicos.sort_unstable();
        unicos.dedup();
        assert_eq!(unicos.len(), todos.len());
        assert!(todos.iter().all(|c| c.starts_with("SESSAO_")));
    }

    #[test]
    fn chave_avulsa_publicada_tem_a_mesma_chave_em_bytes() -> R {
        let mut b = Identidade::nova();
        let avulsa = b.nova_chave_avulsa()?;
        assert_eq!(avulsa.chave_bytes(), avulsa.chave.to_bytes());
        let outra = b.nova_chave_avulsa()?;
        assert_ne!(avulsa.chave_bytes(), outra.chave_bytes());
        Ok(())
    }
}
