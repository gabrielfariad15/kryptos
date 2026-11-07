//! Servidor de repasse do Kryptos.
//!
//! Nesta etapa ele só responde à verificação de saúde. A fila de mensagens
//! cifradas (guardar com validade, entregar e apagar) entra na próxima etapa.
//! Regra 8.2: nada de conteúdo de mensagem, chave ou código em log.

use std::net::SocketAddr;

use axum::{Router, routing::get};

/// Endereço usado quando `KRYPTOS_RELAY_ENDERECO` não está definida.
/// Só a própria máquina: abrir para a rede é decisão de implantação.
pub const ENDERECO_PADRAO: &str = "127.0.0.1:8080";

/// Rotas do servidor.
pub fn rotas() -> Router {
    Router::new().route("/saude", get(saude))
}

async fn saude() -> String {
    format!("ok {}", env!("CARGO_PKG_VERSION"))
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
