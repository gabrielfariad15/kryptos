//! QR do cartão de contato: gerar a grade de módulos (crate `qrcode`) e procurar um QR
//! no quadro de brilho da câmera (crate `rxing`).
//!
//! O quadro vem da câmera e pode ter sido montado por qualquer pessoa. Por isso:
//! - o tamanho é conferido antes de qualquer cópia;
//! - um pânico dentro do leitor vira erro, não derruba o app;
//! - o texto achado não vira contato aqui. Ele ainda passa pela leitura estrita do
//!   cartão (`cartao.rs`), com o código conferido.
//!
//! A tela desenha a grade com `MARGEM_MODULOS` de margem clara em volta, que o padrão
//! do QR exige para a leitura.

use std::collections::HashSet;
use std::panic::{self, AssertUnwindSafe};

use qrcode::{Color, EcLevel, QrCode};
use rxing::{BarcodeFormat, DecodeHints};

use crate::cartao::CartaoContato;

/// Margem clara em volta do QR, em módulos (ISO/IEC 18004 pede 4).
pub const MARGEM_MODULOS: usize = 4;

/// Menor quadro aceito: um QR versão 1 (21 módulos) com 1 pixel por módulo.
pub const LADO_MINIMO: usize = 21;

/// Maior quadro aceito, em pixels: 16 MP. Uma câmera em 1080p manda cerca de 2 MP; o
/// limite só existe para um quadro absurdo não pedir memória sem fim.
pub const PIXELS_MAXIMOS: usize = 4096 * 4096;

/// Correção de erro M (15%): com o cartão de 121 caracteres, a versão 5 (37 módulos),
/// a mesma do nível L. O Q já iria para a versão 7.
const NIVEL: EcLevel = EcLevel::M;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroQr {
    /// Largura, altura ou passo não batem com o tamanho do quadro, ou passam do limite.
    QuadroInvalido,
    /// O gerador não conseguiu montar o QR (não acontece com o cartão de 121 caracteres).
    Geracao,
    /// O leitor entrou em pânico com este quadro.
    Leitor,
}

impl ErroQr {
    pub fn codigo(&self) -> &'static str {
        match self {
            Self::QuadroInvalido => "QUADRO_INVALIDO",
            Self::Geracao => "QR_GERACAO",
            Self::Leitor => "QR_LEITOR",
        }
    }
}

impl std::fmt::Display for ErroQr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErroQr {}

/// Grade quadrada de módulos, linha por linha, sem a margem. `true` é módulo escuro.
#[derive(Clone, PartialEq, Eq)]
pub struct GradeQr {
    lado: usize,
    escuros: Vec<bool>,
}

impl GradeQr {
    pub fn lado(&self) -> usize {
        self.lado
    }

    /// Módulos linha por linha: o de (x, y) está em `y * lado + x`.
    pub fn modulos(&self) -> &[bool] {
        &self.escuros
    }

    pub fn escuro(&self, x: usize, y: usize) -> bool {
        x < self.lado && y < self.lado && self.escuros.get(y * self.lado + x) == Some(&true)
    }
}

impl std::fmt::Debug for GradeQr {
    // A grade é o cartão desenhado: não vai para log (regra 8.2).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GradeQr({}x{}, oculta)", self.lado, self.lado)
    }
}

/// Grade do QR do cartão.
pub fn grade_do_cartao(cartao: &CartaoContato) -> Result<GradeQr, ErroQr> {
    let texto = cartao.como_texto();
    let codigo = QrCode::with_error_correction_level(texto.as_bytes(), NIVEL)
        .map_err(|_| ErroQr::Geracao)?;
    let lado = codigo.width();
    let escuros: Vec<bool> = codigo
        .to_colors()
        .into_iter()
        .map(|c| c == Color::Dark)
        .collect();
    if escuros.len() != lado * lado {
        return Err(ErroQr::Geracao);
    }
    Ok(GradeQr { lado, escuros })
}

/// Procura um QR no quadro de brilho (o plano Y da câmera, 1 byte por pixel).
/// `passo` é o número de bytes de uma linha no buffer, que a câmera pode completar além
/// da largura. Devolve o texto do QR achado, ou `None` se não achou.
pub fn ler_quadro(
    brilho: &[u8],
    largura: usize,
    altura: usize,
    passo: usize,
) -> Result<Option<String>, ErroQr> {
    let luma = copiar_sem_passo(brilho, largura, altura, passo)?;
    let largura = u32::try_from(largura).map_err(|_| ErroQr::QuadroInvalido)?;
    let altura = u32::try_from(altura).map_err(|_| ErroQr::QuadroInvalido)?;
    let mut dicas = DecodeHints {
        PossibleFormats: Some(HashSet::from([BarcodeFormat::QR_CODE])),
        ..Default::default()
    };
    // AssertUnwindSafe: nada do que o fechamento usa fica visível depois de um pânico
    // (a cópia e as dicas são descartadas).
    let resultado = panic::catch_unwind(AssertUnwindSafe(|| {
        rxing::helpers::detect_in_luma_with_hints(
            luma,
            largura,
            altura,
            Some(BarcodeFormat::QR_CODE),
            &mut dicas,
        )
    }))
    .map_err(|_| ErroQr::Leitor)?;
    // Erro do leitor aqui quer dizer "não achou QR legível": é o caso comum de cada
    // quadro sem QR, não uma falha.
    Ok(resultado.ok().map(|r| r.getText().to_string()))
}

/// Confere o tamanho e copia as linhas para um buffer contínuo, largura × altura.
fn copiar_sem_passo(
    brilho: &[u8],
    largura: usize,
    altura: usize,
    passo: usize,
) -> Result<Vec<u8>, ErroQr> {
    if largura < LADO_MINIMO || altura < LADO_MINIMO || passo < largura {
        return Err(ErroQr::QuadroInvalido);
    }
    let pixels = largura
        .checked_mul(altura)
        .filter(|p| *p <= PIXELS_MAXIMOS)
        .ok_or(ErroQr::QuadroInvalido)?;
    // A última linha só precisa da largura, não do passo inteiro.
    let necessario = passo
        .checked_mul(altura - 1)
        .and_then(|n| n.checked_add(largura))
        .ok_or(ErroQr::QuadroInvalido)?;
    if brilho.len() < necessario {
        return Err(ErroQr::QuadroInvalido);
    }
    let mut luma = Vec::with_capacity(pixels);
    for y in 0..altura {
        let inicio = y * passo;
        let linha = brilho
            .get(inicio..inicio + largura)
            .ok_or(ErroQr::QuadroInvalido)?;
        luma.extend_from_slice(linha);
    }
    Ok(luma)
}

#[cfg(test)]
pub(crate) mod testes {
    use std::time::Instant;

    use super::*;
    use crate::identidade::Identidade;

    /// Desenha a grade como uma foto: `px` pixels por módulo, margem de 4 módulos,
    /// girada, com ruído e desfoque. Imita o que a câmera entrega, sem câmera.
    pub(crate) struct Foto {
        pub px: usize,
        pub giro_graus: f64,
        pub ruido: u8,
        pub escuro: u8,
        pub claro: u8,
        pub desfoque: usize,
        pub semente: u64,
    }

    impl Foto {
        pub(crate) fn nitida(px: usize) -> Self {
            Self {
                px,
                giro_graus: 0.0,
                ruido: 0,
                escuro: 0,
                claro: 255,
                desfoque: 0,
                semente: 1,
            }
        }

        /// Devolve (lado, pixels) de um quadro quadrado.
        pub(crate) fn desenhar(&self, grade: &GradeQr) -> (usize, Vec<u8>) {
            let w = grade.lado();
            let lado = (w + 2 * MARGEM_MODULOS) * self.px;
            let tam = lado + lado / 2;
            let (c, s) = (
                self.giro_graus.to_radians().cos(),
                self.giro_graus.to_radians().sin(),
            );
            let centro = tam as f64 / 2.0;
            let meio = lado as f64 / 2.0;
            let mut estado = self.semente.max(1);
            let mut img = vec![self.claro; tam * tam];
            for y in 0..tam {
                for x in 0..tam {
                    let dx = x as f64 - centro;
                    let dy = y as f64 - centro;
                    let ux = c * dx + s * dy + meio;
                    let uy = -s * dx + c * dy + meio;
                    let mut v = self.claro;
                    if ux >= 0.0 && uy >= 0.0 {
                        let mx = ux as usize / self.px;
                        let my = uy as usize / self.px;
                        if mx >= MARGEM_MODULOS
                            && my >= MARGEM_MODULOS
                            && grade.escuro(mx - MARGEM_MODULOS, my - MARGEM_MODULOS)
                        {
                            v = self.escuro;
                        }
                    }
                    let n = if self.ruido > 0 {
                        estado ^= estado << 13;
                        estado ^= estado >> 7;
                        estado ^= estado << 17;
                        (estado % (2 * u64::from(self.ruido) + 1)) as i32 - i32::from(self.ruido)
                    } else {
                        0
                    };
                    img[y * tam + x] = (i32::from(v) + n).clamp(0, 255) as u8;
                }
            }
            (tam, desfocar(tam, &img, self.desfoque))
        }
    }

    fn desfocar(lado: usize, img: &[u8], raio: usize) -> Vec<u8> {
        if raio == 0 {
            return img.to_vec();
        }
        let mut saida = img.to_vec();
        for y in 0..lado {
            for x in 0..lado {
                let (mut soma, mut n) = (0u32, 0u32);
                for yy in y.saturating_sub(raio)..(y + raio + 1).min(lado) {
                    for xx in x.saturating_sub(raio)..(x + raio + 1).min(lado) {
                        soma += u32::from(img[yy * lado + xx]);
                        n += 1;
                    }
                }
                saida[y * lado + x] = (soma / n) as u8;
            }
        }
        saida
    }

    fn cartao_novo() -> CartaoContato {
        CartaoContato::de_chaves(&Identidade::nova().chaves_publicas())
    }

    /// No cartão do vetor. Um cartão sorteado sai na versão 6 (41 módulos) em cerca de
    /// 1 de cada 100: medido 30 de 3.000 em 07/11/2026. Por isso o tamanho é conferido
    /// num cartão fixo.
    #[test]
    fn grade_do_cartao_e_versao_5_com_37_modulos() -> Result<(), Box<dyn std::error::Error>> {
        let grade = grade_do_cartao(&CartaoContato::ler(crate::cartao::testes::CARTAO_DO_VETOR)?)?;
        assert_eq!(grade.lado(), 37);
        assert_eq!(grade.modulos().len(), 37 * 37);
        // Os três quadrados de posição: canto (0,0) escuro, (7,7) claro (separador).
        assert!(grade.escuro(0, 0) && grade.escuro(36, 0) && grade.escuro(0, 36));
        assert!(!grade.escuro(7, 7));
        assert!(!grade.escuro(37, 0), "fora da grade é claro");
        Ok(())
    }

    #[test]
    fn foto_nitida_volta_o_mesmo_texto() -> Result<(), Box<dyn std::error::Error>> {
        for _ in 0..20 {
            let cartao = cartao_novo();
            let (lado, img) = Foto::nitida(4).desenhar(&grade_do_cartao(&cartao)?);
            let lido = ler_quadro(&img, lado, lado, lado)?;
            assert_eq!(lido.as_deref(), Some(cartao.como_texto().as_str()));
            assert_eq!(CartaoContato::ler(&lido.unwrap_or_default())?, cartao);
        }
        Ok(())
    }

    /// Medida, não exigência rígida: imprime quantas fotos degradadas são lidas e falha
    /// só se cair abaixo do que o rxing leu na comparação de 07/11/2026 nas mesmas
    /// condições (com desfoque, 105 de 144).
    #[test]
    fn fotos_degradadas_medida() -> Result<(), ErroQr> {
        let grade = grade_do_cartao(
            &CartaoContato::ler(crate::cartao::testes::CARTAO_DO_VETOR)
                .map_err(|_| ErroQr::Geracao)?,
        )?;
        let esperado = crate::cartao::testes::CARTAO_DO_VETOR;
        let (mut casos, mut lidos, mut semente) = (0u32, 0u32, 0u64);
        for px in [3, 4, 6] {
            for giro in [0.0, 7.0, 20.0, 45.0] {
                for ruido in [0u8, 40, 80] {
                    for (escuro, claro) in [(0u8, 255u8), (60, 170)] {
                        for desfoque in [1, 2] {
                            semente += 1;
                            let foto = Foto {
                                px,
                                giro_graus: giro,
                                ruido,
                                escuro,
                                claro,
                                desfoque,
                                semente,
                            };
                            let (lado, img) = foto.desenhar(&grade);
                            casos += 1;
                            if ler_quadro(&img, lado, lado, lado)?.as_deref() == Some(esperado) {
                                lidos += 1;
                            }
                        }
                    }
                }
            }
        }
        println!("MEDIDA qr degradado: {lidos} de {casos}");
        assert_eq!(casos, 144);
        assert!(lidos >= 100, "leu {lidos} de {casos}");
        Ok(())
    }

    #[test]
    fn passo_maior_que_a_largura_e_respeitado() -> Result<(), Box<dyn std::error::Error>> {
        let cartao = cartao_novo();
        let (lado, img) = Foto::nitida(4).desenhar(&grade_do_cartao(&cartao)?);
        // Linhas com 64 bytes de sobra cheios de lixo escuro: se o passo fosse ignorado,
        // a imagem sairia torta e não leria.
        let passo = lado + 64;
        let mut com_passo = vec![0u8; passo * (lado - 1) + lado];
        for y in 0..lado {
            com_passo[y * passo..y * passo + lado].copy_from_slice(&img[y * lado..(y + 1) * lado]);
        }
        let lido = ler_quadro(&com_passo, lado, lado, passo)?;
        assert_eq!(lido.as_deref(), Some(cartao.como_texto().as_str()));
        Ok(())
    }

    #[test]
    fn quadro_sem_qr_devolve_nada() -> Result<(), ErroQr> {
        assert_eq!(ler_quadro(&vec![200u8; 640 * 480], 640, 480, 640)?, None);
        assert_eq!(ler_quadro(&vec![0u8; 640 * 480], 640, 480, 640)?, None);
        Ok(())
    }

    #[test]
    fn qr_de_outro_app_e_lido_e_recusado_pelo_cartao() -> Result<(), Box<dyn std::error::Error>> {
        // Minúsculas obrigam o modo byte: é o caso de um QR de site comum.
        let codigo =
            QrCode::with_error_correction_level("https://exemplo.com.br/convite?id=42", NIVEL)?;
        let lado = codigo.width();
        let grade = GradeQr {
            lado,
            escuros: codigo
                .to_colors()
                .into_iter()
                .map(|c| c == Color::Dark)
                .collect(),
        };
        let (l, img) = Foto::nitida(4).desenhar(&grade);
        let lido = ler_quadro(&img, l, l, l)?;
        assert_eq!(
            lido.as_deref(),
            Some("https://exemplo.com.br/convite?id=42")
        );
        assert_eq!(
            CartaoContato::ler(&lido.unwrap_or_default()),
            Err(crate::cartao::ErroCartao::NaoEKryptos)
        );
        Ok(())
    }

    #[test]
    fn tamanho_do_quadro_e_conferido_antes_de_ler() {
        let img = vec![0u8; 100 * 100];
        for (l, a, p, motivo) in [
            (0, 100, 100, "largura zero"),
            (100, 0, 100, "altura zero"),
            (20, 100, 100, "abaixo do lado mínimo"),
            (100, 100, 99, "passo menor que a largura"),
            (100, 101, 100, "falta uma linha"),
            (101, 100, 101, "falta um pixel"),
            (usize::MAX, 2, usize::MAX, "multiplicação estoura"),
        ] {
            assert_eq!(
                ler_quadro(&img, l, a, p),
                Err(ErroQr::QuadroInvalido),
                "{motivo}"
            );
        }
        // Acima de 16 MP é recusado mesmo com o buffer do tamanho certo.
        let grande = vec![0u8; 4097 * 4096];
        assert_eq!(
            ler_quadro(&grande, 4097, 4096, 4097),
            Err(ErroQr::QuadroInvalido),
            "acima de 16 MP"
        );
        // A última linha não precisa do passo inteiro.
        assert!(ler_quadro(&img[..99 * 100 + 50], 50, 100, 100).is_ok());
    }

    /// Quadros de lixo de vários tamanhos: o leitor nunca derruba o teste, e nunca acha
    /// um cartão do Kryptos válido.
    #[test]
    fn quadros_aleatorios_nao_derrubam_nem_viram_cartao() -> Result<(), ErroQr> {
        let mut estado = 0x9E37_79B9_7F4A_7C15u64;
        let mut proximo = || {
            estado ^= estado << 13;
            estado ^= estado >> 7;
            estado ^= estado << 17;
            estado
        };
        for _ in 0..60 {
            let largura = LADO_MINIMO + (proximo() % 300) as usize;
            let altura = LADO_MINIMO + (proximo() % 300) as usize;
            let img: Vec<u8> = (0..largura * altura)
                .map(|_| (proximo() >> 56) as u8)
                .collect();
            match ler_quadro(&img, largura, altura, largura) {
                Ok(Some(texto)) => assert!(CartaoContato::ler(&texto).is_err()),
                Ok(None) | Err(ErroQr::Leitor) => {}
                Err(outro) => panic!("erro inesperado {outro:?}"),
            }
        }
        Ok(())
    }

    #[test]
    fn debug_da_grade_nao_mostra_os_modulos() -> Result<(), ErroQr> {
        let grade = grade_do_cartao(&cartao_novo())?;
        let lado = grade.lado();
        assert_eq!(
            format!("{grade:?}"),
            format!("GradeQr({lado}x{lado}, oculta)")
        );
        Ok(())
    }

    /// Tempo de um quadro de câmera comum (1280 × 720) com o QR ocupando um terço da
    /// largura. Só imprime; o número que vale é o medido no aparelho.
    #[test]
    fn tempo_de_um_quadro_medida() -> Result<(), ErroQr> {
        let cartao = cartao_novo();
        let (lado, qr) = Foto::nitida(9).desenhar(&grade_do_cartao(&cartao)?);
        let (l, a) = (1280usize, 720usize);
        let mut quadro = vec![180u8; l * a];
        for y in 0..lado.min(a) {
            quadro[y * l + 400..y * l + 400 + lado].copy_from_slice(&qr[y * lado..(y + 1) * lado]);
        }
        let inicio = Instant::now();
        let lido = ler_quadro(&quadro, l, a, l)?;
        let com_qr = inicio.elapsed();
        let vazio = vec![180u8; l * a];
        let inicio = Instant::now();
        ler_quadro(&vazio, l, a, l)?;
        let sem_qr = inicio.elapsed();
        println!(
            "MEDIDA quadro 1280x720: com QR {} ms, sem QR {} ms (build de teste, computador)",
            com_qr.as_millis(),
            sem_qr.as_millis()
        );
        assert_eq!(lido.as_deref(), Some(cartao.como_texto().as_str()));
        Ok(())
    }
}
