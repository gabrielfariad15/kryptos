//! Estado do aparelho na memória: a identidade desta execução do app.
//!
//! Fica no núcleo para a chave nunca atravessar a ponte: o app só recebe o código.
//! Nada é gravado. Fechar o app perde a identidade (ver `identidade.rs`).

use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::codigo::CodigoContato;
use crate::identidade::Identidade;

static IDENTIDADE: Mutex<Option<Identidade>> = Mutex::new(None);

fn trava() -> MutexGuard<'static, Option<Identidade>> {
    // Trava envenenada quer dizer que outra thread entrou em pânico segurando-a. O valor
    // guardado continua inteiro, porque a única escrita é trocar None por uma identidade
    // pronta, de uma vez. Por isso seguimos com ele em vez de derrubar o app.
    IDENTIDADE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Gera a identidade do aparelho na primeira chamada e devolve o código de contato.
/// Chamadas seguintes devolvem o mesmo código: tocar duas vezes não troca a chave.
pub fn gerar_identidade() -> CodigoContato {
    let mut guarda = trava();
    guarda.get_or_insert_with(Identidade::nova).codigo_contato()
}

#[cfg(test)]
mod testes {
    use super::*;

    // Único teste que toca o estado global, para não disputar com outros em paralelo.
    #[test]
    fn segunda_chamada_devolve_o_mesmo_codigo() {
        let primeiro = gerar_identidade();
        let segundo = gerar_identidade();
        assert_eq!(primeiro, segundo);
    }
}
