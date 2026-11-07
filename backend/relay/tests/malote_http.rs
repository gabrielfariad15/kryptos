//! Malote pelo HTTP, sem abrir porta de rede: A deposita cartas para B sem dizer quem
//! é, B busca com a busca assinada, decifra com o núcleo de verdade e confirma.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use kryptos_nucleo::cartao::CartaoContato;
use kryptos_nucleo::contatos::{Agenda, Contato};
use kryptos_nucleo::identidade::Identidade;
use kryptos_nucleo::sessao::{ChaveAvulsa, Sessao, TIPO_PRIMEIRA, ler_mensagem};
use kryptos_protocolo::carta::{Carta, Tipo};
use kryptos_protocolo::dono_em_texto;
use kryptos_protocolo::malote::{BYTES_MAXIMO_BUSCA, BYTES_MAXIMO_CARTA, Entrega};
use tower::ServiceExt;
use vodozemac::olm::OlmMessage;

type R = Result<(), Box<dyn std::error::Error>>;

async fn pedir(
    rotas: &Router,
    caminho: &str,
    corpo: Vec<u8>,
) -> Result<(StatusCode, Option<String>, Vec<u8>), Box<dyn std::error::Error>> {
    let r = rotas
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(caminho)
                .body(Body::from(corpo))?,
        )
        .await?;
    let status = r.status();
    let tipo = r
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let corpo = r.into_body().collect().await?.to_bytes().to_vec();
    Ok((status, tipo, corpo))
}

fn texto(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn contato(eu: &Identidade, quem: &Identidade) -> Result<Contato, Box<dyn std::error::Error>> {
    let mut agenda = Agenda::nova();
    agenda.adicionar_por_qr(
        &CartaoContato::de_chaves(&quem.chaves_publicas()).como_texto(),
        &eu.chaves_publicas(),
    )?;
    Ok(agenda.contatos()[0])
}

fn carta_para(quem: &Identidade, m: &OlmMessage) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let (tipo, mensagem) = m.to_parts();
    Ok(Carta {
        destinatario: *quem.chaves_publicas().ed25519.as_bytes(),
        tipo: if tipo == TIPO_PRIMEIRA {
            Tipo::Primeira
        } else {
            Tipo::Normal
        },
        mensagem,
    }
    .codificar()?)
}

fn mensagem_da(carta: &[u8]) -> Result<OlmMessage, Box<dyn std::error::Error>> {
    let c = Carta::ler(carta)?;
    Ok(ler_mensagem(usize::from(c.tipo.numero()), &c.mensagem)?)
}

#[tokio::test]
async fn a_deposita_b_busca_decifra_e_confirma() -> R {
    let rotas = kryptos_relay::rotas()?;
    let mut b = Identidade::nova();
    let a = Identidade::nova();

    // B publica; A retira a chave avulsa e abre a conversa.
    pedir(&rotas, "/v1/publicar", b.publicacao(1, 1)?).await?;
    let caminho = format!(
        "/v1/retirar/{}",
        dono_em_texto(b.chaves_publicas().ed25519.as_bytes())
    );
    let (_, _, avulsa) = pedir(&rotas, &caminho, Vec::new()).await?;
    let mut sessao_a = Sessao::abrir(&a, &contato(&a, &b)?, &ChaveAvulsa::da_retirada(&avulsa)?)?;

    // A deposita duas cartas para B. A resposta não diz nada de B.
    for t in [&b"primeira"[..], b"segunda"] {
        let (status, _, corpo) = pedir(
            &rotas,
            "/v1/depositar",
            carta_para(&b, &sessao_a.cifrar(t)?)?,
        )
        .await?;
        assert_eq!(
            (status, texto(&corpo)),
            (StatusCode::OK, "GUARDADA".to_string())
        );
    }

    // Alguém que não é B não consegue buscar: assinatura de outra chave.
    let intruso = Identidade::nova();
    let mut falsa = intruso.busca(1, vec![])?;
    falsa[1..33].copy_from_slice(b.chaves_publicas().ed25519.as_bytes());
    let (status, _, corpo) = pedir(&rotas, "/v1/buscar", falsa).await?;
    assert_eq!(
        (status, texto(&corpo)),
        (StatusCode::FORBIDDEN, "PROTOCOLO_ASSINATURA".to_string())
    );

    // B busca: as duas, na ordem.
    let (status, tipo, corpo) = pedir(&rotas, "/v1/buscar", b.busca(1, vec![])?).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tipo.as_deref(), Some("application/octet-stream"));
    let entrega = Entrega::ler(&corpo)?;
    assert_eq!(entrega.cartas.len(), 2);
    let a_para_b = contato(&b, &a)?;
    let (mut sessao_b, primeira) =
        Sessao::receber_primeira(&mut b, &a_para_b, &mensagem_da(&entrega.cartas[0].1)?)?;
    assert_eq!(primeira.as_slice(), b"primeira");
    assert_eq!(
        sessao_b
            .decifrar(&mensagem_da(&entrega.cartas[1].1)?)?
            .as_slice(),
        b"segunda"
    );

    // A mesma busca copiada e reenviada: recusada pelo número.
    let (status, _, corpo) = pedir(&rotas, "/v1/buscar", b.busca(1, vec![])?).await?;
    assert_eq!(
        (status, texto(&corpo)),
        (StatusCode::CONFLICT, "NUMERO_VELHO 1".to_string())
    );

    // B confirma as duas: a próxima entrega vem vazia.
    let ids = entrega.cartas.iter().map(|(id, _)| *id).collect();
    let (status, _, corpo) = pedir(&rotas, "/v1/buscar", b.busca(2, ids)?).await?;
    assert_eq!(status, StatusCode::OK);
    assert!(Entrega::ler(&corpo)?.cartas.is_empty());
    Ok(())
}

#[tokio::test]
async fn deposito_e_busca_fora_do_formato_sao_recusados() -> R {
    let rotas = kryptos_relay::rotas()?;
    let (status, _, corpo) = pedir(&rotas, "/v1/depositar", vec![1, 2, 3]).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(texto(&corpo).starts_with("PROTOCOLO_"));
    let (status, _, corpo) = pedir(&rotas, "/v1/buscar", vec![1; 10]).await?;
    assert_eq!(
        (status, texto(&corpo)),
        (StatusCode::BAD_REQUEST, "PROTOCOLO_TAMANHO".to_string())
    );
    // Corpos maiores que o limite de cada rota: recusados antes de ler.
    let (status, _, _) = pedir(&rotas, "/v1/depositar", vec![1; BYTES_MAXIMO_CARTA + 1]).await?;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    let (status, _, _) = pedir(&rotas, "/v1/buscar", vec![1; BYTES_MAXIMO_BUSCA + 1]).await?;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    // A maior carta aceita passa pelo limite da rota (acima do limite da publicação).
    let destinatario = Identidade::nova();
    let maior = Carta {
        destinatario: *destinatario.chaves_publicas().ed25519.as_bytes(),
        tipo: Tipo::Normal,
        mensagem: vec![0; kryptos_protocolo::carta::MAXIMO_MENSAGEM],
    }
    .codificar()?;
    assert_eq!(maior.len(), BYTES_MAXIMO_CARTA);
    let (status, _, corpo) = pedir(&rotas, "/v1/depositar", maior).await?;
    assert_eq!(
        (status, texto(&corpo)),
        (StatusCode::OK, "GUARDADA".to_string())
    );
    Ok(())
}

#[tokio::test]
async fn quem_nunca_recebeu_busca_e_recebe_vazio() -> R {
    let rotas = kryptos_relay::rotas()?;
    let b = Identidade::nova();
    let (status, _, corpo) = pedir(&rotas, "/v1/buscar", b.busca(1, vec![])?).await?;
    assert_eq!(status, StatusCode::OK);
    assert!(Entrega::ler(&corpo)?.cartas.is_empty());
    // A mesma busca de novo também: nada foi gravado para quem não tem carta.
    let (status, _, _) = pedir(&rotas, "/v1/buscar", b.busca(1, vec![])?).await?;
    assert_eq!(status, StatusCode::OK);
    Ok(())
}
