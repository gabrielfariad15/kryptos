//! Código de contato: o endereço curto que a pessoa digita, fala ou mostra para ser
//! encontrada. Sai da chave pública da identidade, sem número, e-mail nem cadastro.
//!
//! Derivação v1 (documentacao/arquitetura/codigo-de-contato.md):
//!
//! ```text
//! h       = SHA-256("KRYPTOS/codigo-de-contato/v1" || ed25519 (32 bytes) || curve25519 (32 bytes))
//! dados   = os primeiros 35 bits de h, em 7 caracteres base32 Crockford
//! 8º      = dígito verificador Luhn mod 32 dos 7 caracteres
//! exibido = XXXX-XXXX
//! ```
//!
//! O código é endereço, não prova de identidade. Com 35 bits dá para fabricar uma chave
//! que caia no mesmo código, então quem prova a chave é a conferência pessoal
//! (12 grupos de 4) ou o QR, que leva a chave inteira.
//!
//! Regra 8.2: o código não vai para log nem para mensagem de erro. Por isso o `Debug`
//! esconde o valor, não há `Display`, e os erros de leitura não repetem o texto lido.

use std::fmt;

use sha2::{Digest, Sha256};
use vodozemac::olm::IdentityKeys;

/// Separação de domínio: o mesmo par de chaves dá outro resumo em qualquer outro uso.
/// Mudar a derivação exige trocar o `v1` (e todos os códigos mudam).
const DOMINIO: &[u8] = b"KRYPTOS/codigo-de-contato/v1";

/// Base32 Crockford: sem I, L, O e U, para não confundir com 1, 0 e V.
pub(crate) const ALFABETO: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

const BASE: u32 = 32;

/// Caracteres que vêm do resumo da chave.
pub const CARACTERES_DADOS: usize = 7;

/// Caracteres do código, contando o verificador.
pub const CARACTERES: usize = CARACTERES_DADOS + 1;

/// Código de contato já conferido. Guarda os valores (0 a 31) de cada caractere.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CodigoContato([u8; CARACTERES]);

/// Motivo de um código digitado ser recusado. Não carrega o texto digitado (regra 8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroCodigo {
    /// Quantidade de caracteres diferente de 8, sem contar hífen e espaço.
    Tamanho { encontrados: usize },
    /// Caractere fora do alfabeto, na posição indicada (contada a partir de 1, sem hífen).
    CaractereInvalido { posicao: usize },
    /// O dígito verificador não bate: erro de digitação ou código adulterado.
    Verificador,
}

impl fmt::Display for ErroCodigo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tamanho { encontrados } => write!(
                f,
                "o código tem {CARACTERES} caracteres; foram digitados {encontrados}"
            ),
            Self::CaractereInvalido { posicao } => {
                write!(f, "caractere inválido na posição {posicao}")
            }
            Self::Verificador => write!(f, "código com erro de digitação ou adulterado"),
        }
    }
}

impl std::error::Error for ErroCodigo {}

impl CodigoContato {
    /// Código das chaves públicas de uma identidade.
    pub fn de_chaves(chaves: &IdentityKeys) -> Self {
        Self::de_bytes(chaves.ed25519.as_bytes(), chaves.curve25519.as_bytes())
    }

    /// Código direto dos bytes das chaves. O `cartao` usa para conferir o QR antes de
    /// aceitar a chave Ed25519 (que pode ser inválida no texto lido).
    pub(crate) fn de_bytes(ed25519: &[u8; 32], curve25519: &[u8; 32]) -> Self {
        let mut resumo = Sha256::new();
        resumo.update(DOMINIO);
        resumo.update(ed25519);
        resumo.update(curve25519);
        let resumo = resumo.finalize();

        // Os primeiros 40 bits num u64; os 35 de cima viram os 7 caracteres.
        let mut quarenta: u64 = 0;
        for byte in resumo.iter().take(5) {
            quarenta = (quarenta << 8) | u64::from(*byte);
        }
        let bits = quarenta >> 5;

        let mut valores = [0u8; CARACTERES];
        for (i, valor) in valores.iter_mut().take(CARACTERES_DADOS).enumerate() {
            let deslocamento = 5 * (CARACTERES_DADOS - 1 - i);
            // A máscara deixa só 5 bits: o valor cabe em u8.
            *valor = ((bits >> deslocamento) & 0x1F) as u8;
        }
        valores[CARACTERES_DADOS] = verificador(&valores[..CARACTERES_DADOS]);
        Self(valores)
    }

    /// Lê um código digitado. Aceita minúsculas, hífen e espaços; troca O por 0 e
    /// I ou L por 1, como manda o Crockford. Recusa se o verificador não bater.
    pub fn ler(texto: &str) -> Result<Self, ErroCodigo> {
        let mut valores = [0u8; CARACTERES];
        let mut encontrados = 0usize;
        for c in texto.chars().filter(|c| *c != '-' && !c.is_whitespace()) {
            encontrados += 1;
            let normal = match c.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                outro => outro,
            };
            let valor = ALFABETO
                .iter()
                .position(|a| char::from(*a) == normal)
                .ok_or(ErroCodigo::CaractereInvalido {
                    posicao: encontrados,
                })?;
            if let Some(casa) = valores.get_mut(encontrados - 1) {
                // A posição vem do ALFABETO, de 32 itens: cabe em u8.
                *casa = valor as u8;
            }
        }
        if encontrados != CARACTERES {
            return Err(ErroCodigo::Tamanho { encontrados });
        }
        if !confere(&valores) {
            return Err(ErroCodigo::Verificador);
        }
        Ok(Self(valores))
    }

    /// Texto para mostrar na tela, no formato XXXX-XXXX. Não use em log (regra 8.2).
    pub fn como_texto(&self) -> String {
        let mut texto = String::with_capacity(CARACTERES + 1);
        for (i, valor) in self.0.iter().enumerate() {
            if i == CARACTERES / 2 {
                texto.push('-');
            }
            let simbolo = ALFABETO.get(usize::from(*valor)).copied().unwrap_or(b'?');
            texto.push(char::from(simbolo));
        }
        texto
    }
}

impl fmt::Debug for CodigoContato {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CodigoContato(oculto)")
    }
}

/// Soma do Luhn mod N. `fator_inicial` é 2 para gerar o verificador e 1 para conferir.
fn soma_luhn(valores: &[u8], fator_inicial: u32) -> u32 {
    let mut fator = fator_inicial;
    let mut soma = 0u32;
    for valor in valores.iter().rev() {
        let parcela = fator * u32::from(*valor);
        fator = if fator == 2 { 1 } else { 2 };
        soma += parcela / BASE + parcela % BASE;
    }
    soma
}

/// Dígito verificador Luhn mod 32. Pega toda troca de um caractere e toda troca de
/// dois vizinhos, menos a troca entre 0 e Z (limite conhecido do Luhn mod N).
fn verificador(dados: &[u8]) -> u8 {
    // O resultado é menor que 32: cabe em u8.
    ((BASE - soma_luhn(dados, 2) % BASE) % BASE) as u8
}

fn confere(valores: &[u8]) -> bool {
    soma_luhn(valores, 1).is_multiple_of(BASE)
}

#[cfg(test)]
mod testes {
    use super::*;
    use vodozemac::{Curve25519PublicKey, Ed25519PublicKey};

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

    /// Calculado fora do Rust, por um oráculo em Python (hashlib + Luhn mod 32 escrito à
    /// parte): SHA-256 = 1484fb18 72..., primeiros 35 bits = 2 J 2 F P 6 3, verificador Y.
    const CODIGO_DO_VETOR: &str = "2J2F-P63Y";

    fn do_vetor() -> CodigoContato {
        CodigoContato::de_bytes(&ED25519_RFC8032, &CURVE25519_RFC7748)
    }

    #[test]
    fn vetor_conhecido() {
        assert_eq!(do_vetor().como_texto(), CODIGO_DO_VETOR);
    }

    #[test]
    fn de_chaves_usa_as_duas_chaves_na_ordem_certa() -> Result<(), String> {
        let chaves = IdentityKeys {
            ed25519: Ed25519PublicKey::from_slice(&ED25519_RFC8032).map_err(|e| e.to_string())?,
            curve25519: Curve25519PublicKey::from_bytes(CURVE25519_RFC7748),
        };
        assert_eq!(
            CodigoContato::de_chaves(&chaves).como_texto(),
            CODIGO_DO_VETOR
        );
        // Ordem trocada dá outro código: prova que a ordem faz parte da derivação.
        let trocado = CodigoContato::de_bytes(&CURVE25519_RFC7748, &ED25519_RFC8032);
        assert_ne!(trocado, do_vetor());
        Ok(())
    }

    #[test]
    fn adulterar_um_bit_de_qualquer_chave_muda_o_codigo() {
        let original = do_vetor();
        for byte in 0..32 {
            for bit in 0..8 {
                let mut ed = ED25519_RFC8032;
                ed[byte] ^= 1 << bit;
                assert_ne!(
                    CodigoContato::de_bytes(&ed, &CURVE25519_RFC7748),
                    original,
                    "ed25519 byte {byte} bit {bit}"
                );
                let mut cv = CURVE25519_RFC7748;
                cv[byte] ^= 1 << bit;
                assert_ne!(
                    CodigoContato::de_bytes(&ED25519_RFC8032, &cv),
                    original,
                    "curve25519 byte {byte} bit {bit}"
                );
            }
        }
    }

    #[test]
    fn ler_aceita_o_codigo_certo_em_varias_grafias() {
        let esperado = Ok(do_vetor());
        for grafia in [
            "2J2F-P63Y",
            "2J2FP63Y",
            "2j2f-p63y",
            " 2J2F P63Y ",
            "2J2F--P63Y",
        ] {
            assert_eq!(CodigoContato::ler(grafia), esperado, "grafia {grafia:?}");
        }
    }

    #[test]
    fn ler_troca_o_por_zero_e_i_l_por_um() -> Result<(), ErroCodigo> {
        // Código com 0 e 1 montado pelo próprio verificador.
        let mut valores = [0u8, 1, 0, 1, 2, 3, 4, 0];
        valores[CARACTERES_DADOS] = verificador(&valores[..CARACTERES_DADOS]);
        let codigo = CodigoContato(valores);
        let texto = codigo.como_texto();
        let confuso = texto
            .replace('0', "O")
            .replacen('1', "I", 1)
            .replacen('1', "l", 1);
        assert_ne!(confuso, texto);
        assert_eq!(CodigoContato::ler(&confuso)?, codigo);
        Ok(())
    }

    #[test]
    fn trocar_qualquer_caractere_e_recusado() {
        let texto: Vec<u8> = CODIGO_DO_VETOR.bytes().filter(|b| *b != b'-').collect();
        for posicao in 0..CARACTERES {
            for simbolo in ALFABETO.iter() {
                if *simbolo == texto[posicao] {
                    continue;
                }
                let mut adulterado = texto.clone();
                adulterado[posicao] = *simbolo;
                let adulterado = String::from_utf8_lossy(&adulterado).into_owned();
                assert_eq!(
                    CodigoContato::ler(&adulterado),
                    Err(ErroCodigo::Verificador),
                    "posição {posicao}, símbolo {}",
                    char::from(*simbolo)
                );
            }
        }
    }

    #[test]
    fn trocar_dois_vizinhos_e_recusado() {
        let texto: Vec<u8> = CODIGO_DO_VETOR.bytes().filter(|b| *b != b'-').collect();
        for i in 0..CARACTERES - 1 {
            if texto[i] == texto[i + 1] {
                continue;
            }
            let mut trocado = texto.clone();
            trocado.swap(i, i + 1);
            let trocado = String::from_utf8_lossy(&trocado).into_owned();
            assert_eq!(
                CodigoContato::ler(&trocado),
                Err(ErroCodigo::Verificador),
                "posições {i} e {}",
                i + 1
            );
        }
    }

    /// Calibração (regra 5.3): o caso que o Luhn mod 32 sabidamente deixa passar.
    /// Se este teste falhar, o verificador mudou e o comentário de `verificador` mente.
    #[test]
    fn limite_conhecido_troca_de_0_com_z_passa() {
        let mut valores = [0u8, 31, 5, 6, 7, 8, 9, 0];
        valores[CARACTERES_DADOS] = verificador(&valores[..CARACTERES_DADOS]);
        let mut trocado = valores;
        trocado.swap(0, 1);
        assert!(confere(&valores));
        assert!(confere(&trocado), "0↔Z deveria passar despercebido");
    }

    #[test]
    fn ler_recusa_tamanho_errado() {
        for (texto, encontrados) in [("", 0), ("2J2F-P63", 7), ("2J2F-P63YY", 9)] {
            assert_eq!(
                CodigoContato::ler(texto),
                Err(ErroCodigo::Tamanho { encontrados }),
                "texto {texto:?}"
            );
        }
    }

    #[test]
    fn ler_recusa_caractere_fora_do_alfabeto() {
        assert_eq!(
            CodigoContato::ler("2J2F-P63U"),
            Err(ErroCodigo::CaractereInvalido { posicao: 8 })
        );
        assert_eq!(
            CodigoContato::ler("2J2F-P6#Y"),
            Err(ErroCodigo::CaractereInvalido { posicao: 7 })
        );
        assert_eq!(
            CodigoContato::ler("2J2F-P6ÇY"),
            Err(ErroCodigo::CaractereInvalido { posicao: 7 })
        );
    }

    #[test]
    fn debug_e_erros_nao_mostram_o_codigo() {
        let depuracao = format!("{:?}", do_vetor());
        assert!(!depuracao.contains("2J2F") && !depuracao.contains("P63Y"));

        for digitado in ["2J2F-P63Z", "2J2F-P63U", "2J2F-P63YY"] {
            let erro = match CodigoContato::ler(digitado) {
                Ok(_) => panic!("{digitado} deveria ser recusado"),
                Err(e) => e,
            };
            let mensagem = format!("{erro} {erro:?}");
            assert!(
                !mensagem.contains("2J2F") && !mensagem.contains("P63"),
                "a mensagem repete o código: {mensagem}"
            );
        }
    }

    #[test]
    fn como_texto_tem_hifen_no_meio() {
        let texto = do_vetor().como_texto();
        assert_eq!(texto.len(), CARACTERES + 1);
        assert_eq!(texto.find('-'), Some(CARACTERES / 2));
    }
}
