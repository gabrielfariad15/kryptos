//! Funções do núcleo expostas ao app. Cada uma só repassa para `kryptos_nucleo`.
//!
//! Erros chegam ao Dart como `AnyhowException` com o código fixo do núcleo
//! (`PIN_INCORRETO`, `COFRE_CORROMPIDO`...): nada de PIN, chave ou código de contato.

use std::path::Path;

use kryptos_nucleo::aparelho;
use kryptos_nucleo::cofre::ErroCofre;
use kryptos_nucleo::qr;
use kryptos_nucleo::{Adicao, ChaveAvulsa, CodigoContato, Conferencia};
use zeroize::Zeroizing;

fn erro(e: ErroCofre) -> anyhow::Error {
    anyhow::anyhow!(e.to_string())
}

/// Versão do núcleo compilado dentro do app.
#[flutter_rust_bridge::frb(sync)]
pub fn versao_nucleo() -> String {
    kryptos_nucleo::versao().to_string()
}

/// Gera a identidade do aparelho (uma vez por execução) e devolve o código de
/// contato no formato XXXX-XXXX. A chave fica no núcleo; só o código atravessa.
#[flutter_rust_bridge::frb(sync)]
pub fn gerar_codigo_contato() -> String {
    aparelho::gerar_identidade().como_texto()
}

/// Código da identidade que está na memória, ou nada.
#[flutter_rust_bridge::frb(sync)]
pub fn codigo_atual() -> Option<String> {
    aparelho::codigo_atual().map(|c| c.como_texto())
}

/// Tira a identidade da memória (como fechar o app).
#[flutter_rust_bridge::frb(sync)]
pub fn esquecer_identidade() {
    aparelho::esquecer();
}

#[flutter_rust_bridge::frb(sync)]
pub fn cofre_existe(pasta: String) -> bool {
    kryptos_nucleo::cofre::existe(Path::new(&pasta))
}

/// Segundos que a próxima tentativa de PIN vai esperar antes de conferir.
#[flutter_rust_bridge::frb(sync)]
pub fn cofre_espera_segundos(pasta: String) -> anyhow::Result<u32> {
    aparelho::espera_atual(Path::new(&pasta))
        .map(|d| u32::try_from(d.as_secs()).unwrap_or(u32::MAX))
        .map_err(erro)
}

/// Sal novo e PIN esticado (Argon2id), para o HMAC do Keystore.
pub struct PinPreparado {
    pub sal: Vec<u8>,
    pub esticado: Vec<u8>,
}

/// Criar, etapa 1. O PIN chega como dígitos ASCII.
pub fn cofre_preparar_pin_novo(pin: Vec<u8>) -> anyhow::Result<PinPreparado> {
    let pin = Zeroizing::new(pin);
    let p = aparelho::preparar_pin_novo(&pin).map_err(erro)?;
    Ok(PinPreparado {
        sal: p.sal.to_vec(),
        esticado: p.esticado.to_vec(),
    })
}

/// Criar, etapa 2: grava a identidade da memória com a chave que voltou do Keystore.
pub fn cofre_criar(pasta: String, sal: Vec<u8>, chave: Vec<u8>) -> anyhow::Result<()> {
    let chave = Zeroizing::new(chave);
    aparelho::criar_cofre(Path::new(&pasta), &sal, &chave).map_err(erro)
}

/// Abrir, etapa 1: registra a tentativa, cumpre a espera e estica o PIN.
pub fn cofre_preparar_abertura(pasta: String, pin: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    let pin = Zeroizing::new(pin);
    aparelho::preparar_abertura(Path::new(&pasta), &pin)
        .map(|e| e.to_vec())
        .map_err(erro)
}

/// Abrir, etapa 2: abre com a chave do Keystore e devolve o código de contato.
pub fn cofre_abrir(pasta: String, chave: Vec<u8>) -> anyhow::Result<String> {
    let chave = Zeroizing::new(chave);
    aparelho::abrir_cofre(Path::new(&pasta), &chave)
        .map(|c| c.como_texto())
        .map_err(erro)
}

/// Apaga o arquivo do cofre e o contador (a chave do Keystore o Kotlin apaga).
pub fn cofre_apagar(pasta: String) -> anyhow::Result<()> {
    kryptos_nucleo::cofre::apagar(Path::new(&pasta)).map_err(erro)
}

/// Contagens da busca pelas chaves secretas em disco. Nunca o conteúdo.
pub struct Varredura {
    pub arquivos: u32,
    pub ocorrencias: u32,
    pub ilegiveis: u32,
}

/// Procura as chaves secretas da identidade aberta em todos os arquivos da pasta.
pub fn procurar_segredos(pasta: String) -> anyhow::Result<Varredura> {
    let v = aparelho::procurar_segredos(Path::new(&pasta)).map_err(erro)?;
    Ok(Varredura {
        arquivos: v.arquivos,
        ocorrencias: v.ocorrencias,
        ilegiveis: v.ilegiveis,
    })
}

// ---------- Contatos por QR (documentacao/arquitetura/cartao-de-contato.md) ----------
//
// Só atravessam a ponte a grade do QR próprio (chave pública desenhada), o código de
// contato e a forma como a chave foi conferida. O texto do cartão e as chaves ficam
// no núcleo. Erro chega como o código fixo do núcleo (QR_PROPRIO, QR_NAO_CONFERE...).

/// Grade do QR deste aparelho: `lado` × `lado` módulos, linha por linha, 1 = escuro.
/// A tela desenha a margem clara de `margem` módulos em volta.
pub struct GradeDoQr {
    pub lado: u32,
    pub margem: u32,
    pub modulos: Vec<u8>,
}

/// Grade do QR da identidade na memória, ou nada se não há identidade.
#[flutter_rust_bridge::frb(sync)]
pub fn grade_propria() -> anyhow::Result<Option<GradeDoQr>> {
    let grade = aparelho::grade_propria().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    grade
        .map(|g| {
            Ok(GradeDoQr {
                lado: u32::try_from(g.lado()).map_err(|_| anyhow::anyhow!("QR_GERACAO"))?,
                margem: u32::try_from(qr::MARGEM_MODULOS)
                    .map_err(|_| anyhow::anyhow!("QR_GERACAO"))?,
                modulos: g.modulos().iter().map(|&escuro| u8::from(escuro)).collect(),
            })
        })
        .transpose()
}

/// Contato achado num quadro da câmera.
pub struct ContatoLido {
    pub codigo: String,
    /// `false`: já estava na agenda com as mesmas chaves.
    pub novo: bool,
}

/// Procura um QR no plano de brilho (Y) de um quadro da câmera e, se achar um cartão
/// aceito, põe o contato na agenda. Nada: o quadro não tem QR legível.
/// `passo` é o `bytesPerRow` do plano, que pode passar da largura.
pub fn adicionar_contato_do_quadro(
    brilho: Vec<u8>,
    largura: u32,
    altura: u32,
    passo: u32,
) -> anyhow::Result<Option<ContatoLido>> {
    let medida = |v: u32| usize::try_from(v).map_err(|_| anyhow::anyhow!("QUADRO_INVALIDO"));
    let adicao = aparelho::adicionar_contato_do_quadro(
        &brilho,
        medida(largura)?,
        medida(altura)?,
        medida(passo)?,
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(adicao.map(|a| match a {
        Adicao::Novo(c) => ContatoLido {
            codigo: c.como_texto(),
            novo: true,
        },
        Adicao::JaEstava(c) => ContatoLido {
            codigo: c.como_texto(),
            novo: false,
        },
    }))
}

/// Um contato da agenda desta execução.
pub struct ContatoDaAgenda {
    pub codigo: String,
    /// A chave veio inteira do QR lido pessoalmente.
    pub conferido_por_qr: bool,
}

/// Agenda desta execução, na ordem em que os contatos entraram.
#[flutter_rust_bridge::frb(sync)]
pub fn contatos() -> Vec<ContatoDaAgenda> {
    aparelho::contatos()
        .iter()
        .map(|c| ContatoDaAgenda {
            codigo: c.codigo().como_texto(),
            conferido_por_qr: c.conferencia() == Conferencia::PorQr,
        })
        .collect()
}

// ---------- Conversa e caixa selada (documentacao/arquitetura/caixa-selada.md) ----------
//
// Atravessam a ponte: o cartão próprio (público, é o que vai no QR), a chave avulsa
// pública, cartas cifradas, pedidos e respostas do HMAC do Keystore e, só em
// `caixa_ler`, o texto que a pessoa mandou abrir. Erro chega como o código fixo do
// núcleo.

fn codigo_de(texto: &str) -> anyhow::Result<CodigoContato> {
    CodigoContato::ler(texto).map_err(|e| anyhow::anyhow!(e.to_string()))
}

/// Texto do cartão deste aparelho (o mesmo que vai no QR), ou nada sem identidade.
#[flutter_rust_bridge::frb(sync)]
pub fn cartao_proprio_texto() -> Option<String> {
    aparelho::cartao_proprio().map(|c| c.como_texto())
}

/// Põe na agenda o contato de um cartão já lido em texto (mesmas recusas do QR).
pub fn adicionar_contato_por_texto(texto: String) -> anyhow::Result<ContatoLido> {
    let adicao =
        aparelho::adicionar_contato_por_qr(&texto).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(match adicao {
        Adicao::Novo(c) => ContatoLido {
            codigo: c.como_texto(),
            novo: true,
        },
        Adicao::JaEstava(c) => ContatoLido {
            codigo: c.como_texto(),
            novo: false,
        },
    })
}

/// Chave avulsa nova, já gravada no cofre: 32 bytes de chave e 64 de assinatura, o
/// mesmo formato da retirada no relay.
pub fn nova_chave_avulsa() -> anyhow::Result<Vec<u8>> {
    let a = aparelho::nova_chave_avulsa().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok([a.chave_bytes().as_slice(), a.assinatura_bytes().as_slice()].concat())
}

/// Abre a conversa com um contato da agenda, com a chave avulsa dele.
pub fn abrir_conversa(codigo: String, avulsa: Vec<u8>) -> anyhow::Result<()> {
    let avulsa = ChaveAvulsa::da_retirada(&avulsa).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    aparelho::abrir_conversa(codigo_de(&codigo)?, &avulsa)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

/// Cifra o texto para o contato e devolve a carta, pronta para qualquer transporte.
pub fn cifrar_carta(codigo: String, texto: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    let texto = Zeroizing::new(texto);
    aparelho::cifrar_carta(codigo_de(&codigo)?, &texto).map_err(|e| anyhow::anyhow!(e.to_string()))
}

fn erro_caixa(e: aparelho::ErroCaixa) -> anyhow::Error {
    anyhow::anyhow!(e.to_string())
}

/// O que fazer com uma carta que chegou. `tipo`: SELAR (pedir o HMAC de `pedido` com a
/// chave da `gaveta` e chamar `caixa_selar_chegada`), JA_GUARDADA, NAO_ABRE,
/// NAO_E_PARA_MIM ou CAIXA_CHEIA. Fora de SELAR, `gaveta` é 0 e `pedido` vem vazio.
pub struct ChegadaDaCarta {
    pub tipo: String,
    pub gaveta: u8,
    pub pedido: Vec<u8>,
}

pub fn caixa_receber_carta(carta: Vec<u8>) -> anyhow::Result<ChegadaDaCarta> {
    let simples = |tipo: &str| ChegadaDaCarta {
        tipo: tipo.to_string(),
        gaveta: 0,
        pedido: Vec::new(),
    };
    Ok(match aparelho::receber_carta(&carta).map_err(erro_caixa)? {
        aparelho::Chegada::Selar { gaveta, pedido } => ChegadaDaCarta {
            tipo: "SELAR".to_string(),
            gaveta,
            pedido: pedido.to_vec(),
        },
        aparelho::Chegada::JaGuardada => simples("JA_GUARDADA"),
        aparelho::Chegada::NaoAbre => simples("NAO_ABRE"),
        aparelho::Chegada::NaoEParaMim => simples("NAO_E_PARA_MIM"),
        aparelho::Chegada::CaixaCheia => simples("CAIXA_CHEIA"),
    })
}

/// Sela a carta que esperava o HMAC. Devolve a gaveta.
pub fn caixa_selar_chegada(hmac: Vec<u8>) -> anyhow::Result<u8> {
    let hmac = Zeroizing::new(hmac);
    aparelho::selar_chegada(&hmac).map_err(erro_caixa)
}

pub fn caixa_desistir_da_chegada() -> anyhow::Result<()> {
    aparelho::desistir_da_chegada().map_err(erro_caixa)
}

/// Uma mensagem na caixa, sem nada do conteúdo.
pub struct MensagemNaCaixa {
    pub gaveta: u8,
    pub lida: bool,
    /// Código de quem mandou, ou vazio se não se sabe (caixa antiga, sem remetente).
    pub remetente: String,
}

/// Mensagens não queimadas, na ordem de chegada.
pub fn caixa_listar() -> anyhow::Result<Vec<MensagemNaCaixa>> {
    Ok(aparelho::caixa()
        .map_err(erro_caixa)?
        .into_iter()
        .map(|i| MensagemNaCaixa {
            gaveta: i.gaveta,
            lida: i.lida,
            remetente: i.remetente.map(|c| c.como_texto()).unwrap_or_default(),
        })
        .collect())
}

/// Cartas que não abriram nesta execução do app.
pub fn caixa_nao_abertas() -> anyhow::Result<u32> {
    aparelho::nao_abertas().map_err(erro_caixa)
}

pub fn caixa_pedido_de_leitura(gaveta: u8) -> anyhow::Result<Vec<u8>> {
    Ok(aparelho::pedido_de_leitura(gaveta)
        .map_err(erro_caixa)?
        .to_vec())
}

/// Mensagem aberta para a tela.
pub struct MensagemLida {
    pub remetente: String,
    pub texto: Vec<u8>,
}

/// Abre a mensagem (marca como lida no disco antes). O texto vai para o Dart, que não
/// garante apagar a cópia antes do coletor de lixo passar.
pub fn caixa_ler(gaveta: u8, hmac: Vec<u8>) -> anyhow::Result<MensagemLida> {
    let hmac = Zeroizing::new(hmac);
    let lida = aparelho::ler(gaveta, &hmac).map_err(erro_caixa)?;
    Ok(MensagemLida {
        remetente: lida.remetente.como_texto(),
        texto: lida.texto.to_vec(),
    })
}

/// Gavetas queimadas agora (lidas há 60 s ou mais), para o Kotlin trocar a chave.
pub fn caixa_queimar_vencidas() -> anyhow::Result<Vec<u8>> {
    aparelho::queimar_vencidas().map_err(erro_caixa)
}

/// Gavetas queimadas agora (todas as lidas e as interrompidas), para trocar a chave.
pub fn caixa_queimar_lidas() -> anyhow::Result<Vec<u8>> {
    aparelho::queimar_lidas().map_err(erro_caixa)
}

pub fn caixa_chave_trocada(gaveta: u8) -> anyhow::Result<()> {
    aparelho::chave_trocada(gaveta).map_err(erro_caixa)
}

// ---------- Relay (documentacao/arquitetura/malote.md e estoque-de-chaves.md) ----------
//
// O núcleo monta e assina o que vai para o relay; o Dart só faz o HTTP. Os números
// chegam como i64 (o int do Dart) e não podem ser negativos.

fn numero_de(n: i64) -> anyhow::Result<u64> {
    u64::try_from(n).map_err(|_| anyhow::anyhow!("NUMERO_INVALIDO"))
}

/// Há conversa aberta com o contato nesta execução do app?
#[flutter_rust_bridge::frb(sync)]
pub fn tem_conversa(codigo: String) -> anyhow::Result<bool> {
    Ok(aparelho::tem_conversa(codigo_de(&codigo)?))
}

/// O contato no caminho de `/v1/retirar/{dono}`: a Ed25519 dele em hexadecimal. Pública.
#[flutter_rust_bridge::frb(sync)]
pub fn dono_do_contato(codigo: String) -> anyhow::Result<String> {
    let dono = aparelho::dono_do_contato(codigo_de(&codigo)?)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(kryptos_nucleo::protocolo::dono_em_texto(&dono))
}

/// Publicação assinada com `quantas` chaves avulsas novas (já gravadas no cofre).
/// Com 0, só consulta o estoque.
pub fn relay_publicacao(quantas: u32, numero: i64) -> anyhow::Result<Vec<u8>> {
    let quantas = usize::try_from(quantas).map_err(|_| anyhow::anyhow!("DEMAIS_CHAVES"))?;
    aparelho::preparar_publicacao(quantas, numero_de(numero)?)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

/// Busca assinada para o malote: confirma `confirmadas` (ids de 16 bytes) e pede as
/// próximas cartas.
pub fn relay_busca(numero: i64, confirmadas: Vec<Vec<u8>>) -> anyhow::Result<Vec<u8>> {
    let ids = confirmadas
        .iter()
        .map(|id| <[u8; 16]>::try_from(id.as_slice()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| anyhow::anyhow!("ID_INVALIDO"))?;
    aparelho::preparar_busca(numero_de(numero)?, ids).map_err(|e| anyhow::anyhow!(e.to_string()))
}

/// Uma carta entregue pelo malote, com o id que a confirma.
pub struct CartaEntregue {
    pub id: Vec<u8>,
    pub carta: Vec<u8>,
}

/// Lê a resposta da busca (leitura estrita).
pub fn relay_ler_entrega(entrega: Vec<u8>) -> anyhow::Result<Vec<CartaEntregue>> {
    let e = kryptos_nucleo::protocolo::malote::Entrega::ler(&entrega)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(e.cartas
        .into_iter()
        .map(|(id, carta)| CartaEntregue {
            id: id.to_vec(),
            carta,
        })
        .collect())
}

/// Contagens da busca de um texto (em 8 formas) em todos os arquivos da pasta. Para o
/// teste no aparelho provar que o texto não ficou no disco. Nunca o conteúdo.
pub fn procurar_texto(pasta: String, texto: Vec<u8>) -> anyhow::Result<Varredura> {
    let v = kryptos_nucleo::cofre::procurar(Path::new(&pasta), &[Zeroizing::new(texto)])
        .map_err(erro)?;
    Ok(Varredura {
        arquivos: v.arquivos,
        ocorrencias: v.ocorrencias,
        ilegiveis: v.ilegiveis,
    })
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Desenha a grade como a tela desenha: margem clara, `px` pixels por módulo, e
    /// `sobra` bytes de lixo escuro no fim de cada linha (o `bytesPerRow` da câmera).
    fn foto(g: &GradeDoQr, px: usize, sobra: usize) -> (Vec<u8>, u32, u32) {
        let (lado, margem) = (g.lado as usize, g.margem as usize);
        let largura = (lado + 2 * margem) * px;
        let passo = largura + sobra;
        let mut img = vec![0u8; passo * largura];
        for y in 0..largura {
            for x in 0..largura {
                let (mx, my) = (x / px, y / px);
                let dentro =
                    (margem..margem + lado).contains(&mx) && (margem..margem + lado).contains(&my);
                let escuro = dentro && g.modulos[(my - margem) * lado + (mx - margem)] == 1;
                img[y * passo + x] = if escuro { 0 } else { 255 };
            }
        }
        (img, largura as u32, passo as u32)
    }

    fn codigo_do_erro(r: anyhow::Result<Option<ContatoLido>>) -> String {
        match r {
            Err(e) => e.to_string(),
            Ok(_) => "sem erro".to_string(),
        }
    }

    /// O estado do aparelho é global e os testes do cargo rodam em paralelo: quem mexe
    /// nele pega esta trava.
    static SERIE: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn em_serie() -> std::sync::MutexGuard<'static, ()> {
        SERIE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn grade_quadro_e_agenda_pela_ponte() -> anyhow::Result<()> {
        let _serie = em_serie();
        esquecer_identidade();
        assert!(grade_propria()?.is_none(), "grade sem identidade");

        // Identidade A mostra o QR.
        let codigo_a = gerar_codigo_contato();
        let grade_a = grade_propria()?.ok_or_else(|| anyhow::anyhow!("sem grade"))?;
        assert_eq!((grade_a.lado, grade_a.margem), (37, 4));
        assert_eq!(grade_a.modulos.len(), 37 * 37);
        assert!(grade_a.modulos.iter().all(|&m| m <= 1));
        let (foto_a, lado, passo) = foto(&grade_a, 4, 64);

        // Identidade B lê o QR de A.
        esquecer_identidade();
        let codigo_b = gerar_codigo_contato();
        assert_ne!(codigo_a, codigo_b);
        let lido = adicionar_contato_do_quadro(foto_a.clone(), lado, lado, passo)?
            .ok_or_else(|| anyhow::anyhow!("não leu"))?;
        assert_eq!((lido.codigo.as_str(), lido.novo), (codigo_a.as_str(), true));
        let de_novo = adicionar_contato_do_quadro(foto_a.clone(), lado, lado, passo)?
            .ok_or_else(|| anyhow::anyhow!("não leu de novo"))?;
        assert!(!de_novo.novo, "o mesmo QR não pode entrar duas vezes");
        let agenda = contatos();
        assert_eq!(agenda.len(), 1);
        assert_eq!(agenda[0].codigo, codigo_a);
        assert!(agenda[0].conferido_por_qr);

        // Sem QR: nada. Passo ignorado (= largura): imagem torta, não lê.
        assert!(adicionar_contato_do_quadro(vec![200; 300 * 300], 300, 300, 300)?.is_none());
        assert!(adicionar_contato_do_quadro(foto_a.clone(), lado, lado, lado)?.is_none());

        // O próprio QR é recusado com o código fixo.
        let (foto_b, lado_b, passo_b) = foto(
            &grade_propria()?.ok_or_else(|| anyhow::anyhow!("sem grade"))?,
            4,
            0,
        );
        assert_eq!(
            codigo_do_erro(adicionar_contato_do_quadro(foto_b, lado_b, lado_b, passo_b)),
            "QR_PROPRIO"
        );
        // Quadro menor do que dizem largura, altura e passo.
        assert_eq!(
            codigo_do_erro(adicionar_contato_do_quadro(vec![0; 10], lado, lado, passo)),
            "QUADRO_INVALIDO"
        );
        assert_eq!(contatos().len(), 1, "recusa não pode mexer na agenda");

        // "Fechar o app" esvazia a agenda.
        esquecer_identidade();
        assert!(contatos().is_empty());
        Ok(())
    }

    /// Faz o papel do Keystore: SHA-256 de um nome de chave e dos dados.
    fn keystore_falso(chave: &str, dados: &[u8]) -> Vec<u8> {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(chave.as_bytes());
        h.update(dados);
        h.finalize().to_vec()
    }

    fn abrir_cofre_pela_ponte(pasta: &str) -> anyhow::Result<String> {
        let esticado = cofre_preparar_abertura(pasta.to_string(), b"246801".to_vec())?;
        cofre_abrir(pasta.to_string(), keystore_falso("cofre", &esticado))
    }

    /// O caminho que o teste no aparelho faz, sem o celular: B cria o cofre e publica
    /// uma chave avulsa; A abre a conversa e manda duas cartas; B recebe, sela, lê e
    /// queima. O texto não fica no disco.
    #[test]
    fn caixa_pela_ponte() -> anyhow::Result<()> {
        let _serie = em_serie();
        let pasta_b =
            std::env::temp_dir().join(format!("kryptos-ponte-caixa-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta_b);
        std::fs::create_dir_all(&pasta_b)?;
        let pasta = pasta_b.to_string_lossy().to_string();
        let r = (|| -> anyhow::Result<()> {
            // B
            esquecer_identidade();
            gerar_codigo_contato();
            let p = cofre_preparar_pin_novo(b"246801".to_vec())?;
            cofre_criar(pasta.clone(), p.sal, keystore_falso("cofre", &p.esticado))?;
            let cartao_b = cartao_proprio_texto().ok_or_else(|| anyhow::anyhow!("sem cartão"))?;
            let avulsa = nova_chave_avulsa()?;
            assert_eq!(avulsa.len(), 96);

            // A, sem cofre
            esquecer_identidade();
            gerar_codigo_contato();
            let b = adicionar_contato_por_texto(cartao_b)?;
            abrir_conversa(b.codigo.clone(), avulsa)?;
            let texto = "MENSAGEM-PELA-PONTE-ação".as_bytes().to_vec();
            let carta = cifrar_carta(b.codigo.clone(), texto.clone())?;
            let cartao_a = cartao_proprio_texto().ok_or_else(|| anyhow::anyhow!("sem cartão"))?;
            let codigo_a = codigo_atual().ok_or_else(|| anyhow::anyhow!("sem código"))?;
            esquecer_identidade();

            // B de novo
            abrir_cofre_pela_ponte(&pasta)?;
            adicionar_contato_por_texto(cartao_a)?;
            assert!(caixa_queimar_lidas()?.is_empty());
            let c = caixa_receber_carta(carta.clone())?;
            assert_eq!(c.tipo, "SELAR");
            assert_eq!(c.pedido.len(), 42);
            let gaveta =
                caixa_selar_chegada(keystore_falso(&format!("gaveta {}", c.gaveta), &c.pedido))?;
            assert_eq!(gaveta, c.gaveta);
            assert_eq!(caixa_receber_carta(carta.clone())?.tipo, "JA_GUARDADA");
            let mut outra = carta.clone();
            outra[1] ^= 1;
            assert_eq!(caixa_receber_carta(outra)?.tipo, "NAO_E_PARA_MIM");
            let erro = match caixa_receber_carta(vec![1, 2, 3]) {
                Err(e) => e.to_string(),
                Ok(_) => "sem erro".to_string(),
            };
            assert!(erro.starts_with("PROTOCOLO_"), "erro {erro}");

            let lista = caixa_listar()?;
            assert_eq!(lista.len(), 1);
            assert!(!lista[0].lida);
            assert_eq!(lista[0].remetente, codigo_a, "remetente antes de abrir");
            assert_eq!(procurar_texto(pasta.clone(), texto.clone())?.ocorrencias, 0);

            let pedido = caixa_pedido_de_leitura(gaveta)?;
            let lida = caixa_ler(gaveta, keystore_falso(&format!("gaveta {gaveta}"), &pedido))?;
            assert_eq!(lida.remetente, codigo_a);
            assert_eq!(lida.texto, texto);
            assert!(caixa_listar()?[0].lida);

            assert_eq!(caixa_queimar_lidas()?, vec![gaveta]);
            caixa_chave_trocada(gaveta)?;
            assert!(caixa_listar()?.is_empty());
            assert_eq!(caixa_nao_abertas()?, 0);
            let v = procurar_texto(pasta.clone(), texto.clone())?;
            assert_eq!((v.arquivos, v.ocorrencias, v.ilegiveis), (104, 0, 0));
            // calibração: em claro, a busca acha
            std::fs::write(pasta_b.join("vazamento"), &texto)?;
            assert!(procurar_texto(pasta.clone(), texto)?.ocorrencias >= 1);
            Ok(())
        })();
        esquecer_identidade();
        std::fs::remove_dir_all(&pasta_b)?;
        r
    }

    /// O que vai para o relay sai assinado e no formato; a entrega volta lida.
    #[test]
    fn relay_pela_ponte() -> anyhow::Result<()> {
        use kryptos_nucleo::protocolo::malote::{Busca, Entrega};
        use kryptos_nucleo::protocolo::{dono_em_texto, Publicacao};
        let _serie = em_serie();
        let pasta_b =
            std::env::temp_dir().join(format!("kryptos-ponte-relay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta_b);
        std::fs::create_dir_all(&pasta_b)?;
        let pasta = pasta_b.to_string_lossy().to_string();
        let r = (|| -> anyhow::Result<()> {
            esquecer_identidade();
            gerar_codigo_contato();
            // sem cofre não publica (a chave avulsa se perderia)
            assert!(relay_publicacao(1, 1).is_err());
            let p = cofre_preparar_pin_novo(b"246801".to_vec())?;
            cofre_criar(pasta.clone(), p.sal, keystore_falso("cofre", &p.esticado))?;
            let publicacao = Publicacao::conferir(&relay_publicacao(3, 42)?)?;
            assert_eq!((publicacao.numero, publicacao.chaves.len()), (42, 3));
            assert!(relay_publicacao(1, -1).is_err(), "número negativo");

            let busca = Busca::conferir(&relay_busca(7, vec![vec![1; 16], vec![2; 16]])?)?;
            assert_eq!(busca.numero, 7);
            assert_eq!(busca.confirmadas, vec![[1; 16], [2; 16]]);
            assert_eq!(busca.destinatario, publicacao.dono);
            assert!(relay_busca(8, vec![vec![1; 15]]).is_err(), "id de 15 bytes");

            let entrega = Entrega {
                cartas: vec![([5; 16], vec![9; 40])],
            }
            .codificar()?;
            let lidas = relay_ler_entrega(entrega)?;
            assert_eq!(lidas.len(), 1);
            assert_eq!(
                (lidas[0].id.clone(), lidas[0].carta.clone()),
                (vec![5; 16], vec![9; 40])
            );
            assert!(relay_ler_entrega(vec![1, 0]).is_err(), "entrega cortada");

            // o dono de um contato é a Ed25519 dele, em hexadecimal
            let cartao_b = cartao_proprio_texto().ok_or_else(|| anyhow::anyhow!("sem cartão"))?;
            esquecer_identidade();
            gerar_codigo_contato();
            let b = adicionar_contato_por_texto(cartao_b)?;
            assert_eq!(
                dono_do_contato(b.codigo.clone())?,
                dono_em_texto(&publicacao.dono)
            );
            assert!(!tem_conversa(b.codigo.clone())?);
            assert!(dono_do_contato("0000-0000".to_string()).is_err());
            Ok(())
        })();
        esquecer_identidade();
        std::fs::remove_dir_all(&pasta_b)?;
        r
    }
}
