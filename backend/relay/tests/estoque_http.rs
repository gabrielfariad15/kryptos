//! Estoque de chaves pelo HTTP, sem abrir porta de rede: publicar, retirar e o caminho
//! inteiro até a conversa abrir, com o núcleo de verdade dos dois lados.

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use http_body_util::BodyExt;
use kryptos_nucleo::cartao::CartaoContato;
use kryptos_nucleo::contatos::{Agenda, Contato};
use kryptos_nucleo::identidade::Identidade;
use kryptos_nucleo::sessao::{ChaveAvulsa, Sessao};
use kryptos_protocolo::{BYTES_MAXIMO_PUBLICACAO, dono_em_texto};
use tower::ServiceExt;

type R = Result<(), Box<dyn std::error::Error>>;

struct Resposta {
    status: StatusCode,
    cabecalhos: HeaderMap,
    corpo: Vec<u8>,
}

impl Resposta {
    fn texto(&self) -> String {
        String::from_utf8_lossy(&self.corpo).into_owned()
    }
}

async fn pedir(
    rotas: &Router,
    metodo: &str,
    caminho: &str,
    corpo: Vec<u8>,
) -> Result<Resposta, Box<dyn std::error::Error>> {
    let r = rotas
        .clone()
        .oneshot(
            Request::builder()
                .method(metodo)
                .uri(caminho)
                .body(Body::from(corpo))?,
        )
        .await?;
    let status = r.status();
    let cabecalhos = r.headers().clone();
    let corpo = r.into_body().collect().await?.to_bytes().to_vec();
    Ok(Resposta {
        status,
        cabecalhos,
        corpo,
    })
}

fn caminho_de_retirada(b: &Identidade) -> String {
    format!(
        "/v1/retirar/{}",
        dono_em_texto(b.chaves_publicas().ed25519.as_bytes())
    )
}

/// `quem` visto por `eu`, lido do QR.
fn contato(eu: &Identidade, quem: &Identidade) -> Result<Contato, Box<dyn std::error::Error>> {
    let mut agenda = Agenda::nova();
    agenda.adicionar_por_qr(
        &CartaoContato::de_chaves(&quem.chaves_publicas()).como_texto(),
        &eu.chaves_publicas(),
    )?;
    Ok(agenda.contatos()[0])
}

#[tokio::test]
async fn b_publica_a_retira_e_a_conversa_abre() -> R {
    let rotas = kryptos_relay::rotas();
    let mut b = Identidade::nova();
    let a = Identidade::nova();

    let r = pedir(&rotas, "POST", "/v1/publicar", b.publicacao(3, 1)?).await?;
    assert_eq!(
        (r.status, r.texto()),
        (StatusCode::OK, "ESTOQUE 3".to_string())
    );

    let r = pedir(&rotas, "POST", &caminho_de_retirada(&b), Vec::new()).await?;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(
        r.cabecalhos.get(header::CONTENT_TYPE).map(|v| v.as_bytes()),
        Some(&b"application/octet-stream"[..])
    );
    let avulsa = ChaveAvulsa::da_retirada(&r.corpo)?;
    let m = Sessao::abrir(&a, &contato(&a, &b)?, &avulsa)?.cifrar(b"pelo relay")?;
    let a_para_b = contato(&b, &a)?;
    let (_, lido) = Sessao::receber_primeira(&mut b, &a_para_b, &m)?;
    assert_eq!(lido.as_slice(), b"pelo relay");

    // Consulta: sobraram 2.
    let r = pedir(&rotas, "POST", "/v1/publicar", b.publicacao(0, 2)?).await?;
    assert_eq!(r.texto(), "ESTOQUE 2");
    Ok(())
}

#[tokio::test]
async fn cada_chave_sai_uma_vez_e_depois_falta() -> R {
    let rotas = kryptos_relay::rotas();
    let mut b = Identidade::nova();
    pedir(&rotas, "POST", "/v1/publicar", b.publicacao(2, 1)?).await?;
    let um = pedir(&rotas, "POST", &caminho_de_retirada(&b), Vec::new()).await?;
    let dois = pedir(&rotas, "POST", &caminho_de_retirada(&b), Vec::new()).await?;
    assert_eq!((um.status, dois.status), (StatusCode::OK, StatusCode::OK));
    assert_ne!(um.corpo, dois.corpo);
    let tres = pedir(&rotas, "POST", &caminho_de_retirada(&b), Vec::new()).await?;
    assert_eq!(
        (tres.status, tres.texto()),
        (StatusCode::NOT_FOUND, "SEM_CHAVE".to_string())
    );
    // Dono que nunca publicou: a mesma resposta.
    let outro = Identidade::nova();
    let r = pedir(&rotas, "POST", &caminho_de_retirada(&outro), Vec::new()).await?;
    assert_eq!(
        (r.status, r.texto()),
        (StatusCode::NOT_FOUND, "SEM_CHAVE".to_string())
    );
    Ok(())
}

/// A publicação copiada e reenviada é recusada pelo número.
#[tokio::test]
async fn publicacao_repetida_e_recusada_e_devolve_o_ultimo_numero() -> R {
    let rotas = kryptos_relay::rotas();
    let mut b = Identidade::nova();
    let publicacao = b.publicacao(1, 7)?;
    pedir(&rotas, "POST", "/v1/publicar", publicacao.clone()).await?;
    let r = pedir(&rotas, "POST", "/v1/publicar", publicacao).await?;
    assert_eq!(
        (r.status, r.texto()),
        (StatusCode::CONFLICT, "NUMERO_VELHO 7".to_string())
    );
    let r = pedir(&rotas, "POST", "/v1/publicar", b.publicacao(1, 8)?).await?;
    assert_eq!(r.texto(), "ESTOQUE 2");
    Ok(())
}

#[tokio::test]
async fn publicacao_adulterada_ou_fora_do_formato_e_recusada() -> R {
    let rotas = kryptos_relay::rotas();
    let mut b = Identidade::nova();
    let original = b.publicacao(1, 1)?;
    // Posição 33: primeiro byte do número (depois da versão e dos 32 do dono).
    for (posicao, esperado) in [
        (0usize, "PROTOCOLO_VERSAO"),
        (33, "PROTOCOLO_ASSINATURA"),
        (original.len() - 1, "PROTOCOLO_ASSINATURA"),
    ] {
        let mut ruim = original.clone();
        ruim[posicao] ^= 1;
        let r = pedir(&rotas, "POST", "/v1/publicar", ruim).await?;
        assert_eq!(
            (r.status, r.texto()),
            (StatusCode::BAD_REQUEST, esperado.to_string()),
            "byte {posicao}"
        );
    }
    // Um bit trocado no dono pode dar um ponto fora da curva ou outra chave válida,
    // conforme a chave sorteada: as duas recusas servem.
    let mut ruim = original.clone();
    ruim[5] ^= 1;
    let r = pedir(&rotas, "POST", "/v1/publicar", ruim).await?;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert!(
        ["PROTOCOLO_CHAVE_INVALIDA", "PROTOCOLO_ASSINATURA"].contains(&r.texto().as_str()),
        "{}",
        r.texto()
    );
    let r = pedir(&rotas, "POST", "/v1/publicar", Vec::new()).await?;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = pedir(
        &rotas,
        "POST",
        "/v1/publicar",
        vec![1; BYTES_MAXIMO_PUBLICACAO + 1],
    )
    .await?;
    assert_eq!(r.status, StatusCode::PAYLOAD_TOO_LARGE);
    // Nenhuma recusa pôs nada no estoque.
    let r = pedir(&rotas, "POST", &caminho_de_retirada(&b), Vec::new()).await?;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn limite_por_destinatario_responde_429_com_espera() -> R {
    let rotas = kryptos_relay::rotas();
    let mut b = Identidade::nova();
    pedir(&rotas, "POST", "/v1/publicar", b.publicacao(20, 1)?).await?;
    for _ in 0..kryptos_relay::estoque::RETIRADAS_SEGUIDAS {
        let r = pedir(&rotas, "POST", &caminho_de_retirada(&b), Vec::new()).await?;
        assert_eq!(r.status, StatusCode::OK);
    }
    let r = pedir(&rotas, "POST", &caminho_de_retirada(&b), Vec::new()).await?;
    assert_eq!(
        (r.status, r.texto()),
        (StatusCode::TOO_MANY_REQUESTS, "LIMITE".to_string())
    );
    let espera: u64 = r
        .cabecalhos
        .get(header::RETRY_AFTER)
        .ok_or("sem Retry-After")?
        .to_str()?
        .parse()?;
    assert!(
        (1..=kryptos_relay::estoque::INTERVALO_DA_FICHA.as_secs()).contains(&espera),
        "Retry-After {espera}"
    );
    Ok(())
}

#[tokio::test]
async fn dono_fora_do_formato_e_metodo_errado() -> R {
    let rotas = kryptos_relay::rotas();
    let r = pedir(&rotas, "POST", "/v1/retirar/abc", Vec::new()).await?;
    assert_eq!(
        (r.status, r.texto()),
        (StatusCode::BAD_REQUEST, "PROTOCOLO_DONO".to_string())
    );
    let b = Identidade::nova();
    let r = pedir(&rotas, "GET", &caminho_de_retirada(&b), Vec::new()).await?;
    assert_eq!(r.status, StatusCode::METHOD_NOT_ALLOWED);
    let r = pedir(&rotas, "GET", "/v1/publicar", Vec::new()).await?;
    assert_eq!(r.status, StatusCode::METHOD_NOT_ALLOWED);
    Ok(())
}
