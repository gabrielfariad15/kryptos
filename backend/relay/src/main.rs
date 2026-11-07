use std::process::ExitCode;

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

async fn rodar() -> Result<(), String> {
    let valor = std::env::var("KRYPTOS_RELAY_ENDERECO").ok();
    let endereco = kryptos_relay::endereco(valor.as_deref())?;
    let ouvinte = tokio::net::TcpListener::bind(endereco)
        .await
        .map_err(|e| format!("não consegui escutar em {endereco}: {e}"))?;
    tracing::info!("relay escutando em {endereco}");
    axum::serve(ouvinte, kryptos_relay::rotas())
        .with_graceful_shutdown(desligar())
        .await
        .map_err(|e| format!("servidor parou com erro: {e}"))?;
    tracing::info!("relay desligado");
    Ok(())
}

async fn desligar() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        tracing::error!("não consegui ouvir o Ctrl+C: {e}");
    }
}
