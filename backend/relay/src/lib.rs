//! Servidor de repasse do Kryptos.
//!
//! - `GET /saude`: verificação de saúde.
//! - `POST /v1/publicar`: o dono põe chaves avulsas no estoque (corpo: publicação do
//!   `kryptos_protocolo`, assinada pelo dono).
//! - `POST /v1/retirar/{dono}`: entrega uma chave avulsa do dono, uma vez só.
//! - `POST /v1/depositar`: guarda uma carta para o destinatário dela (corpo: a carta).
//! - `POST /v1/buscar`: o destinatário confirma as cartas já guardadas e recebe as
//!   próximas (corpo: busca assinada).
//!
//! Formato e decisões: documentacao/arquitetura/estoque-de-chaves.md e malote.md.
//!
//! Regra 8.2: nada de chave, dono ou conteúdo em log. O relay não lê o IP de ninguém;
//! o limite por endereço de rede fica no proxy da implantação.

pub mod estoque;
pub mod malote;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use kryptos_protocolo::carta::Carta;
use kryptos_protocolo::malote::{BYTES_MAXIMO_BUSCA, BYTES_MAXIMO_CARTA, Busca};
use kryptos_protocolo::{BYTES_MAXIMO_PUBLICACAO, ErroProtocolo, Publicacao, dono_de_texto};

use crate::estoque::{ErroEstoque, Estoque};
use crate::malote::{ErroMalote, Malote};

/// Endereço usado quando `KRYPTOS_RELAY_ENDERECO` não está definida.
/// Só a própria máquina: abrir para a rede é decisão de implantação.
pub const ENDERECO_PADRAO: &str = "127.0.0.1:8080";

#[derive(Clone)]
struct Compartilhado {
    estoque: Arc<Mutex<Estoque>>,
    malote: Arc<Malote>,
}

fn trava(estado: &Compartilhado) -> MutexGuard<'_, Estoque> {
    // Trava envenenada: outro pedido entrou em pânico segurando-a. Cada mudança no
    // estoque termina inteira ou não começa, então o valor guardado continua válido.
    estado
        .estoque
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Rotas do servidor, com estoque e malote novos e vazios, só na memória.
pub fn rotas() -> Result<Router, ErroMalote> {
    Ok(rotas_com_malote(Arc::new(Malote::em_memoria()?)))
}

/// Rotas do servidor com o malote dado (o de arquivo, no relay de verdade).
pub fn rotas_com_malote(malote: Arc<Malote>) -> Router {
    rotas_com(Compartilhado {
        estoque: Arc::new(Mutex::new(Estoque::novo())),
        malote,
    })
}

fn rotas_com(estado: Compartilhado) -> Router {
    Router::new()
        .route("/saude", get(saude))
        .route(
            "/v1/publicar",
            post(publicar).layer(DefaultBodyLimit::max(BYTES_MAXIMO_PUBLICACAO)),
        )
        .route("/v1/retirar/{dono}", post(retirar))
        .route(
            "/v1/depositar",
            post(depositar).layer(DefaultBodyLimit::max(BYTES_MAXIMO_CARTA)),
        )
        .route(
            "/v1/buscar",
            post(buscar).layer(DefaultBodyLimit::max(BYTES_MAXIMO_BUSCA)),
        )
        .layer(DefaultBodyLimit::max(BYTES_MAXIMO_PUBLICACAO))
        .with_state(estado)
}

/// Segundos Unix agora. O malote usa o relógio de parede: a validade de 7 dias tem de
/// valer depois de o relay reiniciar, e o relógio monotônico recomeça do zero.
pub fn segundos_agora() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn resposta_do_malote(e: ErroMalote) -> Response {
    match e {
        ErroMalote::NumeroVelho { ultimo } => {
            texto(StatusCode::CONFLICT, format!("{} {ultimo}", e.codigo()))
        }
        ErroMalote::MaloteCheio => texto(StatusCode::CONFLICT, e.codigo().to_string()),
        ErroMalote::RelayCheio => texto(StatusCode::SERVICE_UNAVAILABLE, e.codigo().to_string()),
        ErroMalote::Disco | ErroMalote::Aleatorio => {
            texto(StatusCode::INTERNAL_SERVER_ERROR, e.codigo().to_string())
        }
    }
}

fn resposta_do_protocolo(e: ErroProtocolo) -> Response {
    let status = if e == ErroProtocolo::Assinatura {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::BAD_REQUEST
    };
    texto(status, e.codigo().to_string())
}

/// Resposta 200: `GUARDADA`. O banco grava fora da thread do servidor.
async fn depositar(State(estado): State<Compartilhado>, corpo: Bytes) -> Response {
    let carta = match Carta::ler(&corpo) {
        Ok(c) => c,
        Err(e) => return resposta_do_protocolo(e),
    };
    let agora = segundos_agora();
    let malote = Arc::clone(&estado.malote);
    match tokio::task::spawn_blocking(move || malote.depositar(&carta, &corpo, agora)).await {
        Ok(Ok(())) => texto(StatusCode::OK, "GUARDADA".to_string()),
        Ok(Err(e)) => resposta_do_malote(e),
        Err(_) => texto(StatusCode::INTERNAL_SERVER_ERROR, "INTERNO".to_string()),
    }
}

/// Resposta 200: a entrega (`kryptos_protocolo::malote::Entrega`), talvez vazia.
async fn buscar(State(estado): State<Compartilhado>, corpo: Bytes) -> Response {
    let busca = match Busca::conferir(&corpo) {
        Ok(b) => b,
        Err(e) => return resposta_do_protocolo(e),
    };
    let agora = segundos_agora();
    let malote = Arc::clone(&estado.malote);
    match tokio::task::spawn_blocking(move || malote.buscar(&busca, agora)).await {
        Ok(Ok(entrega)) => match entrega.codificar() {
            Ok(b) => (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/octet-stream")],
                b,
            )
                .into_response(),
            Err(e) => {
                tracing::error!("malote: entrega fora do formato ({})", e.codigo());
                texto(StatusCode::INTERNAL_SERVER_ERROR, "INTERNO".to_string())
            }
        },
        Ok(Err(e)) => resposta_do_malote(e),
        Err(_) => texto(StatusCode::INTERNAL_SERVER_ERROR, "INTERNO".to_string()),
    }
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
