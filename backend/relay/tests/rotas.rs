//! Testes das rotas pela interface HTTP, sem abrir porta de rede.

// Arquivo inteiro e teste: unwrap aqui derruba o teste com a mensagem do erro (regra 7.1 isenta).
#![allow(clippy::unwrap_used)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn pedir(metodo: &str, caminho: &str) -> (StatusCode, String) {
    let resposta = kryptos_relay::rotas()
        .unwrap()
        .oneshot(
            Request::builder()
                .method(metodo)
                .uri(caminho)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resposta.status();
    let corpo = resposta.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(corpo.to_vec()).unwrap())
}

#[tokio::test]
async fn saude_responde_ok_com_versao() {
    let (status, corpo) = pedir("GET", "/saude").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(corpo, format!("ok {}", env!("CARGO_PKG_VERSION")));
}

#[tokio::test]
async fn rota_inexistente_da_404() {
    let (status, _) = pedir("GET", "/mensagens").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn saude_so_aceita_get() {
    let (status, _) = pedir("POST", "/saude").await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}
