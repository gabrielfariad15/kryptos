//! Cartão de contato: o texto que vai dentro do QR. Leva a chave pública inteira, e
//! por isso ler o QR pessoalmente confere a chave (o código de contato sozinho não
//! confere: tem só 35 bits).
//!
//! Formato v1 (documentacao/arquitetura/cartao-de-contato.md):
//!
//! ```text
//! KRYPTOS1-<chaves>-<código>
//!   chaves = ed25519 (32 bytes) || curve25519 (32 bytes), em base32 Crockford
//!            maiúsculo: 512 bits + 3 bits zero = 103 caracteres
//!   código = o código de contato dessas chaves, sem hífen (8 caracteres)
//! ```
//!
//! São 121 caracteres, todos do modo alfanumérico do QR. O código no fim confere o
//! texto inteiro (35 bits do SHA-256 mais o verificador) e é o mesmo que a outra
//! pessoa vê na tela dela.
//!
//! A leitura é estrita: sem minúscula, sem O por 0, sem espaço. O texto vem de um QR,
//! não de digitação, e um só texto por cartão impede duas grafias da mesma chave.
//!
//! Regra 8.2: o `Debug` esconde as chaves, não há `Display`, e os erros não repetem o
//! texto lido.

use std::fmt;

use vodozemac::olm::IdentityKeys;
use vodozemac::{Curve25519PublicKey, Ed25519PublicKey};

use crate::codigo::{ALFABETO, CARACTERES as CARACTERES_CODIGO, CodigoContato};

/// Começo do texto. Mudar o formato exige trocar o `1`.
pub const PREFIXO: &str = "KRYPTOS1";

/// Começo comum a qualquer versão: separa "QR de outra versão" de "QR de outro app".
const FAMILIA: &str = "KRYPTOS";

const BYTES_CHAVES: usize = 64;

/// 512 bits em grupos de 5: 103 caracteres, o último com 2 bits de dado e 3 zero.
pub const CARACTERES_CHAVES: usize = 103;

/// Tamanho total: prefixo, hífen, chaves, hífen, código.
pub const CARACTERES: usize = PREFIXO.len() + 1 + CARACTERES_CHAVES + 1 + CARACTERES_CODIGO;

/// Chaves públicas lidas de um QR e conferidas.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CartaoContato {
    chaves: IdentityKeys,
}

/// Motivo de um QR ser recusado. Não carrega o texto lido (regra 8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroCartao {
    /// Não começa por KRYPTOS: QR de outro app, site, Wi-Fi...
    NaoEKryptos,
    /// Começa por KRYPTOS mas não é a versão 1: app desatualizado ou QR de versão nova.
    VersaoDesconhecida,
    /// Tamanho diferente de 121.
    Tamanho,
    /// Separador fora do lugar ou caractere fora do alfabeto (inclui minúscula).
    Formato,
    /// A chave Ed25519 não é um ponto válido da curva.
    ChaveInvalida,
    /// O código do fim não é o das chaves: QR danificado ou adulterado.
    NaoConfere,
}

impl ErroCartao {
    /// Código fixo que atravessa a ponte e vira texto na tela.
    pub fn codigo(&self) -> &'static str {
        match self {
            Self::NaoEKryptos => "QR_NAO_E_KRYPTOS",
            Self::VersaoDesconhecida => "QR_VERSAO_DESCONHECIDA",
            Self::Tamanho => "QR_TAMANHO",
            Self::Formato => "QR_FORMATO",
            Self::ChaveInvalida => "QR_CHAVE_INVALIDA",
            Self::NaoConfere => "QR_NAO_CONFERE",
        }
    }
}

impl fmt::Display for ErroCartao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErroCartao {}

impl CartaoContato {
    pub fn de_chaves(chaves: &IdentityKeys) -> Self {
        Self { chaves: *chaves }
    }

    pub fn chaves(&self) -> &IdentityKeys {
        &self.chaves
    }

    pub fn codigo(&self) -> CodigoContato {
        CodigoContato::de_chaves(&self.chaves)
    }

    /// Texto do QR. Não use em log (regra 8.2).
    pub fn como_texto(&self) -> String {
        montar(
            self.chaves.ed25519.as_bytes(),
            self.chaves.curve25519.as_bytes(),
        )
    }

    /// Lê o texto de um QR. Confere, nesta ordem: família, versão, tamanho, formato,
    /// o código do fim contra as chaves e, por último, a chave Ed25519.
    pub fn ler(texto: &str) -> Result<Self, ErroCartao> {
        if !texto.starts_with(FAMILIA) {
            return Err(ErroCartao::NaoEKryptos);
        }
        let resto = texto
            .strip_prefix(PREFIXO)
            .and_then(|r| r.strip_prefix('-'))
            .ok_or(ErroCartao::VersaoDesconhecida)?;
        if texto.len() != CARACTERES {
            return Err(ErroCartao::Tamanho);
        }
        // Daqui em diante o texto é ASCII de tamanho certo, ou é recusado por formato.
        let (corpo, codigo) = resto.split_once('-').ok_or(ErroCartao::Formato)?;
        if corpo.len() != CARACTERES_CHAVES || codigo.len() != CARACTERES_CODIGO {
            return Err(ErroCartao::Formato);
        }
        let bytes = decodificar(corpo.as_bytes())?;
        if !codigo.bytes().all(|b| ALFABETO.contains(&b)) {
            return Err(ErroCartao::Formato);
        }

        let (ed, cv) = bytes.split_at(32);
        let ed: [u8; 32] = ed.try_into().map_err(|_| ErroCartao::Formato)?;
        let cv: [u8; 32] = cv.try_into().map_err(|_| ErroCartao::Formato)?;

        // O código vem antes da chave: um QR danificado diz "não confere", não
        // "chave inválida", que soaria como problema de quem gerou.
        let esperado = CodigoContato::de_bytes(&ed, &cv)
            .como_texto()
            .replace('-', "");
        if esperado != codigo {
            return Err(ErroCartao::NaoConfere);
        }
        let ed25519 = Ed25519PublicKey::from_slice(&ed).map_err(|_| ErroCartao::ChaveInvalida)?;
        Ok(Self {
            chaves: IdentityKeys {
                ed25519,
                curve25519: Curve25519PublicKey::from_bytes(cv),
            },
        })
    }
}

impl fmt::Debug for CartaoContato {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CartaoContato(oculto)")
    }
}

fn montar(ed25519: &[u8; 32], curve25519: &[u8; 32]) -> String {
    let mut dados = [0u8; BYTES_CHAVES];
    dados[..32].copy_from_slice(ed25519);
    dados[32..].copy_from_slice(curve25519);
    let codigo = CodigoContato::de_bytes(ed25519, curve25519)
        .como_texto()
        .replace('-', "");
    let mut texto = String::with_capacity(CARACTERES);
    texto.push_str(PREFIXO);
    texto.push('-');
    texto.push_str(&codificar(&dados));
    texto.push('-');
    texto.push_str(&codigo);
    texto
}

/// 64 bytes em 103 caracteres base32 Crockford, do bit mais alto para o mais baixo.
fn codificar(dados: &[u8; BYTES_CHAVES]) -> String {
    let mut texto = String::with_capacity(CARACTERES_CHAVES);
    let mut acumulado: u16 = 0;
    let mut bits = 0u32;
    for byte in dados {
        acumulado = (acumulado << 8) | u16::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            texto.push(simbolo((acumulado >> bits) & 0x1F));
        }
        // Só os bits ainda não usados ficam no acumulador (cabem em u16).
        acumulado &= (1 << bits) - 1;
    }
    if bits > 0 {
        texto.push(simbolo((acumulado << (5 - bits)) & 0x1F));
    }
    texto
}

fn simbolo(valor: u16) -> char {
    char::from(ALFABETO.get(usize::from(valor)).copied().unwrap_or(b'?'))
}

/// O inverso de `codificar`. Recusa símbolo fora do alfabeto e bits de enchimento
/// diferentes de zero (senão haveria mais de um texto para as mesmas chaves).
fn decodificar(texto: &[u8]) -> Result<[u8; BYTES_CHAVES], ErroCartao> {
    if texto.len() != CARACTERES_CHAVES {
        return Err(ErroCartao::Formato);
    }
    let mut dados = [0u8; BYTES_CHAVES];
    let mut acumulado: u16 = 0;
    let mut bits = 0u32;
    let mut i = 0usize;
    for c in texto {
        let valor = ALFABETO
            .iter()
            .position(|a| a == c)
            .ok_or(ErroCartao::Formato)?;
        // A posição vem do ALFABETO, de 32 itens: cabe em u16.
        acumulado = (acumulado << 5) | valor as u16;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            let casa = dados.get_mut(i).ok_or(ErroCartao::Formato)?;
            // A máscara deixa só 8 bits: cabe em u8.
            *casa = ((acumulado >> bits) & 0xFF) as u8;
            i += 1;
            acumulado &= (1 << bits) - 1;
        }
    }
    if i != BYTES_CHAVES || acumulado != 0 {
        return Err(ErroCartao::Formato);
    }
    Ok(dados)
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;
    use crate::identidade::Identidade;

    /// Chave pública Ed25519 do teste 1 da RFC 8032.
    const ED25519_RFC8032: [u8; 32] = [
        0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64, 0x07,
        0x3a, 0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68, 0xf7, 0x07,
        0x51, 0x1a,
    ];

    /// Chave pública X25519 da Alice, seção 6.1 da RFC 7748.
    const CURVE25519_RFC7748: [u8; 32] = [
        0x85, 0x20, 0xf0, 0x09, 0x89, 0x30, 0xa7, 0x54, 0x74, 0x8b, 0x7d, 0xdc, 0xb4, 0x3e, 0xf7,
        0x5a, 0x0d, 0xbf, 0x3a, 0x0d, 0x26, 0x38, 0x1a, 0xf4, 0xeb, 0xa4, 0xa9, 0x8e, 0xaa, 0x9b,
        0x4e, 0x6a,
    ];

    /// Calculado fora do Rust por ferramentas/oraculos/cartao.py (biblioteca padrão do
    /// Python, base32 e Luhn escritos à parte; calibrado pelo código 2J2F-P63Y).
    pub(crate) const CARTAO_DO_VETOR: &str = "KRYPTOS1-TXD9G0C2P45BFNABZV9WJS07787E2WQKVAK269DF08D6HXR7A4D8A87G164K19TMEJ5QVQ5M7VVNM3DZ786JCE0TYKNT9ACENADMWTG-2J2FP63Y";

    fn chaves_do_vetor() -> Result<IdentityKeys, String> {
        Ok(IdentityKeys {
            ed25519: Ed25519PublicKey::from_slice(&ED25519_RFC8032).map_err(|e| e.to_string())?,
            curve25519: Curve25519PublicKey::from_bytes(CURVE25519_RFC7748),
        })
    }

    #[test]
    fn vetor_conhecido() -> Result<(), String> {
        let cartao = CartaoContato::de_chaves(&chaves_do_vetor()?);
        assert_eq!(cartao.como_texto(), CARTAO_DO_VETOR);
        assert_eq!(CARTAO_DO_VETOR.len(), CARACTERES);
        assert_eq!(cartao.codigo().como_texto(), "2J2F-P63Y");
        Ok(())
    }

    #[test]
    fn ler_o_vetor_devolve_as_mesmas_chaves() -> Result<(), Box<dyn std::error::Error>> {
        let lido = CartaoContato::ler(CARTAO_DO_VETOR)?;
        assert_eq!(lido.chaves(), &chaves_do_vetor()?);
        Ok(())
    }

    #[test]
    fn ida_e_volta_com_identidades_novas() -> Result<(), ErroCartao> {
        for _ in 0..200 {
            let identidade = Identidade::nova();
            let cartao = CartaoContato::de_chaves(&identidade.chaves_publicas());
            let texto = cartao.como_texto();
            assert_eq!(texto.len(), CARACTERES);
            let lido = CartaoContato::ler(&texto)?;
            assert_eq!(lido, cartao);
            assert_eq!(lido.codigo(), identidade.codigo_contato());
        }
        Ok(())
    }

    #[test]
    fn so_caracteres_do_modo_alfanumerico_do_qr() {
        // Modo alfanumérico do QR (ISO/IEC 18004): 0-9, A-Z, espaço e $%*+-./:
        let permitidos = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";
        for _ in 0..50 {
            let texto =
                CartaoContato::de_chaves(&Identidade::nova().chaves_publicas()).como_texto();
            assert!(texto.chars().all(|c| permitidos.contains(c)));
        }
    }

    /// Adulteração (regra 8.4): cada caractere depois do prefixo, trocado por cada
    /// outro símbolo do alfabeto, é recusado. Nenhuma troca passa.
    #[test]
    fn qualquer_caractere_trocado_e_recusado() {
        let original: Vec<u8> = CARTAO_DO_VETOR.bytes().collect();
        let mut recusas = [0usize; 6];
        let mut casos = 0usize;
        for posicao in PREFIXO.len() + 1..original.len() {
            if original[posicao] == b'-' {
                continue;
            }
            for simbolo in ALFABETO.iter() {
                if *simbolo == original[posicao] {
                    continue;
                }
                let mut adulterado = original.clone();
                adulterado[posicao] = *simbolo;
                let texto = String::from_utf8_lossy(&adulterado).into_owned();
                casos += 1;
                match CartaoContato::ler(&texto) {
                    Ok(_) => panic!("posição {posicao}, símbolo {} passou", char::from(*simbolo)),
                    Err(ErroCartao::NaoConfere) => recusas[0] += 1,
                    Err(ErroCartao::Formato) => recusas[1] += 1,
                    Err(ErroCartao::ChaveInvalida) => recusas[2] += 1,
                    Err(outro) => panic!("posição {posicao}: erro inesperado {outro:?}"),
                }
            }
        }
        assert_eq!(casos, (CARACTERES_CHAVES + CARACTERES_CODIGO) * 31);
        // Só a última posição das chaves dá Formato: dos 31 símbolos possíveis ali, 28
        // mexem nos bits de enchimento. Em todas as outras, o código do fim pega a troca
        // antes da chave ser examinada, então nenhuma chega a ChaveInvalida.
        assert_eq!(recusas, [casos - 28, 28, 0, 0, 0, 0], "recusas por tipo");
    }

    #[test]
    fn qr_de_outro_app_e_recusado_como_tal() {
        for texto in [
            "",
            "https://kryptos.example/convite",
            "WIFI:S:casa;T:WPA;P:senha;;",
            "kryptos1-",
            "2J2F-P63Y",
        ] {
            assert_eq!(
                CartaoContato::ler(texto),
                Err(ErroCartao::NaoEKryptos),
                "{texto:?}"
            );
        }
    }

    #[test]
    fn outra_versao_e_recusada_como_versao() {
        let v2 = CARTAO_DO_VETOR.replacen("KRYPTOS1", "KRYPTOS2", 1);
        assert_eq!(CartaoContato::ler(&v2), Err(ErroCartao::VersaoDesconhecida));
        assert_eq!(
            CartaoContato::ler("KRYPTOS"),
            Err(ErroCartao::VersaoDesconhecida)
        );
        let sem_hifen = CARTAO_DO_VETOR.replacen("KRYPTOS1-", "KRYPTOS1", 1);
        assert_eq!(
            CartaoContato::ler(&sem_hifen),
            Err(ErroCartao::VersaoDesconhecida)
        );
    }

    #[test]
    fn tamanho_errado_e_recusado() {
        let curto = &CARTAO_DO_VETOR[..CARACTERES - 1];
        let longo = format!("{CARTAO_DO_VETOR}Y");
        let com_espaco = format!("{CARTAO_DO_VETOR} ");
        for texto in [curto, longo.as_str(), com_espaco.as_str(), "KRYPTOS1-"] {
            assert_eq!(
                CartaoContato::ler(texto),
                Err(ErroCartao::Tamanho),
                "{texto:?}"
            );
        }
    }

    #[test]
    fn leitura_estrita_recusa_minuscula_e_letras_confusas() {
        let corpo = PREFIXO.len() + 1;
        for (posicao, troca) in [
            (corpo, "t"),
            (corpo, "U"),
            (corpo + 3, "O"),
            (corpo + 4, "I"),
            (CARACTERES - 1, "y"),
            (CARACTERES - 1, "#"),
        ] {
            let mut texto = CARTAO_DO_VETOR.to_string();
            texto.replace_range(posicao..posicao + 1, troca);
            assert_eq!(
                CartaoContato::ler(&texto),
                Err(ErroCartao::Formato),
                "posição {posicao}, {troca}"
            );
        }
    }

    #[test]
    fn separador_fora_do_lugar_e_recusado() {
        let mut texto = CARTAO_DO_VETOR.to_string();
        let segundo_hifen = PREFIXO.len() + 1 + CARACTERES_CHAVES;
        // Troca o hífen do meio com o caractere ao lado: tamanho igual, partes erradas.
        texto.replace_range(segundo_hifen - 1..segundo_hifen + 1, "-T");
        assert_eq!(CartaoContato::ler(&texto), Err(ErroCartao::Formato));
    }

    #[test]
    fn bits_de_enchimento_diferentes_de_zero_sao_recusados() {
        let ultima = PREFIXO.len() + CARACTERES_CHAVES;
        let original = CARTAO_DO_VETOR.as_bytes()[ultima];
        let valor = ALFABETO.iter().position(|a| *a == original).unwrap_or(0);
        // Os 3 bits de baixo do último símbolo das chaves são enchimento.
        for enchimento in 1..8 {
            let novo = ALFABETO[(valor & !0b111) | enchimento];
            let mut texto = CARTAO_DO_VETOR.to_string();
            texto.replace_range(ultima..ultima + 1, &char::from(novo).to_string());
            assert_eq!(
                CartaoContato::ler(&texto),
                Err(ErroCartao::Formato),
                "enchimento {enchimento}"
            );
        }
    }

    #[test]
    fn chave_ed25519_fora_da_curva_e_recusada_mesmo_com_codigo_certo() {
        // Procura 32 bytes que não são ponto da curva (cerca de metade não é).
        let invalida = (0u8..=255)
            .map(|b| {
                let mut k = [0u8; 32];
                k[0] = b;
                k[1] = 0x80;
                k
            })
            .find(|k| Ed25519PublicKey::from_slice(k).is_err());
        let Some(invalida) = invalida else {
            panic!("não achei chave Ed25519 inválida para o teste");
        };
        // Montado com o código certo: só a chave pode ser o motivo da recusa.
        let texto = montar(&invalida, &CURVE25519_RFC7748);
        assert_eq!(CartaoContato::ler(&texto), Err(ErroCartao::ChaveInvalida));
    }

    #[test]
    fn debug_e_erros_nao_mostram_chave_nem_texto() -> Result<(), String> {
        let chaves = chaves_do_vetor()?;
        let cartao = CartaoContato::de_chaves(&chaves);
        let depuracao = format!("{cartao:?}");
        for proibido in [
            chaves.ed25519.to_base64(),
            chaves.curve25519.to_base64(),
            "TXD9G0".to_string(),
            "2J2F".to_string(),
        ] {
            assert!(!depuracao.contains(&proibido), "Debug mostra {proibido}");
        }
        let mut adulterado = CARTAO_DO_VETOR.to_string();
        adulterado.replace_range(CARACTERES - 1..CARACTERES, "Z");
        let erro = match CartaoContato::ler(&adulterado) {
            Ok(_) => return Err("o adulterado passou".into()),
            Err(e) => e,
        };
        let mensagem = format!("{erro} {erro:?}");
        assert!(!mensagem.contains("TXD9G0") && !mensagem.contains("2J2F"));
        Ok(())
    }
}
