use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use kryptos_relay::malote::Malote;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    match rodar().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(erro) => {
            tracing::error!("{erro}");
            ExitCode::FAILURE
        }
    }
}

/// Intervalo da varredura das cartas vencidas.
const INTERVALO_DA_LIMPEZA: Duration = Duration::from_secs(600);

async fn rodar() -> Result<(), String> {
    let valor = std::env::var("KRYPTOS_RELAY_ENDERECO").ok();
    let endereco = kryptos_relay::endereco(valor.as_deref())?;
    let malote = Arc::new(abrir_malote()?);
    let ouvinte = tokio::net::TcpListener::bind(endereco)
        .await
        .map_err(|e| format!("não consegui escutar em {endereco}: {e}"))?;
    tracing::info!("relay escutando em {endereco}");
    tokio::spawn(limpar_de_tempos_em_tempos(Arc::clone(&malote)));
    axum::serve(ouvinte, kryptos_relay::rotas_com_malote(malote))
        .with_graceful_shutdown(desligar())
        .await
        .map_err(|e| format!("servidor parou com erro: {e}"))?;
    tracing::info!("relay desligado");
    Ok(())
}

/// Malote no arquivo de `KRYPTOS_RELAY_MALOTE`. Sem a variável, só na memória, com
/// aviso no log: as cartas não sobrevivem a um reinício.
fn abrir_malote() -> Result<Malote, String> {
    match std::env::var_os("KRYPTOS_RELAY_MALOTE") {
        Some(caminho) => {
            let m = Malote::em_arquivo(std::path::Path::new(&caminho))
                .map_err(|e| format!("não consegui abrir o malote ({e})"))?;
            tracing::info!("malote em arquivo");
            Ok(m)
        }
        None => {
            tracing::warn!(
                "KRYPTOS_RELAY_MALOTE não definida: malote só na memória, as cartas somem se o relay reiniciar"
            );
            Malote::em_memoria().map_err(|e| format!("não consegui criar o malote ({e})"))
        }
    }
}

async fn limpar_de_tempos_em_tempos(malote: Arc<Malote>) {
    let mut relogio = tokio::time::interval(INTERVALO_DA_LIMPEZA);
    loop {
        relogio.tick().await;
        let m = Arc::clone(&malote);
        let agora = kryptos_relay::segundos_agora();
        match tokio::task::spawn_blocking(move || m.limpar(agora)).await {
            Ok(Ok(0)) => {}
            Ok(Ok(n)) => tracing::info!("malote: {n} cartas vencidas apagadas"),
            Ok(Err(e)) => tracing::error!("malote: limpeza falhou ({e})"),
            Err(e) => tracing::error!("malote: limpeza parou ({e})"),
        }
    }
}

async fn desligar() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        tracing::error!("não consegui ouvir o Ctrl+C: {e}");
    }
}
