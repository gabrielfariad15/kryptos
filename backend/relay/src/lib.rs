//! Servidor de repasse do Kryptos.
//!
//! - `GET /saude`: verificação de saúde.
//! - `POST /v1/publicar`: o dono põe chaves avulsas no estoque (corpo: publicação do
//!   `kryptos_protocolo`, assinada pelo dono).
//! - `POST /v1/retirar/{dono}`: entrega uma chave avulsa do dono, uma vez só.
//!
//! Formato e decisões: documentacao/arquitetura/estoque-de-chaves.md. A fila de
//! mensagens cifradas entra numa etapa própria.
//!
//! Regra 8.2: nada de chave, dono ou conteúdo em log. O relay não lê o IP de ninguém;
//! o limite por endereço de rede fica no proxy da implantação.

pub mod estoque;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use kryptos_protocolo::{BYTES_MAXIMO_PUBLICACAO, Publicacao, dono_de_texto};

use crate::estoque::{ErroEstoque, Estoque};

/// Endereço usado quando `KRYPTOS_RELAY_ENDERECO` não está definida.
/// Só a própria máquina: abrir para a rede é decisão de implantação.
pub const ENDERECO_PADRAO: &str = "127.0.0.1:8080";

type Compartilhado = Arc<Mutex<Estoque>>;

fn trava(estado: &Compartilhado) -> MutexGuard<'_, Estoque> {
    // Trava envenenada: outro pedido entrou em pânico segurando-a. Cada mudança no
    // estoque termina inteira ou não começa, então o valor guardado continua válido.
    estado.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Rotas do servidor, com um estoque novo e vazio.
pub fn rotas() -> Router {
    rotas_com(Arc::new(Mutex::new(Estoque::novo())))
}

fn rotas_com(estado: Compartilhado) -> Router {
    Router::new()
        .route("/saude", get(saude))
        .route("/v1/publicar", post(publicar))
        .route("/v1/retirar/{dono}", post(retirar))
        .layer(DefaultBodyLimit::max(BYTES_MAXIMO_PUBLICACAO))
        .with_state(estado)
}

async fn saude() -> String {
    format!("ok {}", env!("CARGO_PKG_VERSION"))
}

fn texto(status: StatusCode, corpo: String) -> Response {
    (status, corpo).into_response()
}

fn resposta_de_erro(e: ErroEstoque) -> Response {
    match e {
        ErroEstoque::NumeroVelho { ultimo } => {
            texto(StatusCode::CONFLICT, format!("{} {ultimo}", e.codigo()))
        }
        ErroEstoque::EstoqueCheio { estoque } => {
            texto(StatusCode::CONFLICT, format!("{} {estoque}", e.codigo()))
        }
        ErroEstoque::ChaveRepetida => texto(StatusCode::CONFLICT, e.codigo().to_string()),
        ErroEstoque::RelayCheio => texto(StatusCode::SERVICE_UNAVAILABLE, e.codigo().to_string()),
        ErroEstoque::SemChave => texto(StatusCode::NOT_FOUND, e.codigo().to_string()),
        ErroEstoque::Limite { espera } => {
            // Arredonda para cima: tentar no segundo indicado já encontra a ficha.
            let segundos = espera.as_secs() + u64::from(espera.subsec_nanos() > 0);
            let mut r = texto(StatusCode::TOO_MANY_REQUESTS, e.codigo().to_string());
            if let Ok(v) = HeaderValue::from_str(&segundos.to_string()) {
                r.headers_mut().insert(header::RETRY_AFTER, v);
            }
            r
        }
    }
}

/// Resposta 200: `ESTOQUE <quantas o dono tem agora>`.
async fn publicar(State(estado): State<Compartilhado>, corpo: Bytes) -> Response {
    let publicacao = match Publicacao::conferir(&corpo) {
        Ok(p) => p,
        Err(e) => return texto(StatusCode::BAD_REQUEST, e.codigo().to_string()),
    };
    let agora = Instant::now();
    match trava(&estado).publicar(publicacao, agora) {
        Ok(n) => texto(StatusCode::OK, format!("ESTOQUE {n}")),
        Err(e) => resposta_de_erro(e),
    }
}

/// Resposta 200: a chave assinada, 96 bytes.
async fn retirar(State(estado): State<Compartilhado>, Path(dono): Path<String>) -> Response {
    let dono = match dono_de_texto(&dono) {
        Ok(d) => d,
        Err(e) => return texto(StatusCode::BAD_REQUEST, e.codigo().to_string()),
    };
    let agora = Instant::now();
    match trava(&estado).retirar(&dono, agora) {
        Ok(chave) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/octet-stream")],
            chave.para_bytes().to_vec(),
        )
            .into_response(),
        Err(e) => resposta_de_erro(e),
    }
}

/// Lê o endereço de escuta. Valor inválido é erro, nunca cai no padrão calado.
pub fn endereco(valor: Option<&str>) -> Result<SocketAddr, String> {
    let texto = valor.unwrap_or(ENDERECO_PADRAO);
    texto
        .parse::<SocketAddr>()
        .map_err(|e| format!("KRYPTOS_RELAY_ENDERECO inválido ({texto:?}): {e}"))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn endereco_padrao_quando_nao_definido() {
        assert_eq!(endereco(None).unwrap().to_string(), ENDERECO_PADRAO);
    }

    #[test]
    fn endereco_definido_e_respeitado() {
        assert_eq!(endereco(Some("0.0.0.0:9000")).unwrap().port(), 9000);
    }

    #[test]
    fn endereco_invalido_e_recusado() {
        for ruim in ["", "localhost", "127.0.0.1", "127.0.0.1:99999", "abc:80"] {
            assert!(endereco(Some(ruim)).is_err(), "aceitou {ruim:?}");
        }
    }
}
