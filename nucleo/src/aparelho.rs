//! Estado do aparelho na memória: a identidade desta execução do app, a agenda de
//! contatos, as conversas e as etapas do cofre que o app chama pela ponte.
//!
//! Fica no núcleo para a chave nunca atravessar a ponte: o app recebe o código de
//! contato e, no cofre, só o PIN esticado e a chave do Keystore passam por ele.
//!
//! Criar (primeira abertura):
//!   `gerar_identidade` → `preparar_pin_novo` → HMAC no Keystore → `criar_cofre`
//! Abrir (aberturas seguintes):
//!   `preparar_abertura` → HMAC no Keystore → `abrir_cofre`
//! Contatos (gravados no cofre, documentacao/arquitetura/agenda-gravada.md):
//!   `grade_propria` para desenhar o QR; `adicionar_contato_do_quadro` com cada quadro
//!   da câmera (ou `adicionar_contato_por_qr` com o texto já lido). Com o cofre aberto,
//!   o contato só entra na memória depois de a agenda nova estar gravada e lida de
//!   volta. Antes de o cofre existir, fica só na memória e vai junto na criação.
//! Conversas (só em memória; documentacao/arquitetura/sessao-olm.md):
//!   `nova_chave_avulsa` para B publicar; `abrir_conversa` e `cifrar` em A; `receber`
//!   nos dois lados.
//! Caixa selada (documentacao/arquitetura/caixa-selada.md), com o cofre aberto:
//!   `receber_carta` → HMAC no Keystore com a chave da gaveta → `selar_chegada`;
//!   `caixa` lista; `pedido_de_leitura` → HMAC → `ler`; `queimar_vencidas` (a cada
//!   segundo) e `queimar_lidas` (ao abrir, ao sair do app e ao apagar a tela) devolvem
//!   as gavetas cuja chave o Kotlin troca; depois, `chave_trocada`.
//!
//! Com o cofre aberto, a chave do cofre fica na memória junto da conta, para regravar
//! a conta quando ela muda: chave avulsa gerada ou gasta. Regra de ordem: o disco
//! primeiro. Uma chave avulsa só sai daqui depois de gravada e lida de volta; o texto
//! de uma primeira mensagem só sai depois de a chave gasta estar gravada como gasta.
//!
//! Uma trava só: identidade, selo, agenda e conversas pertencem à mesma identidade e
//! mudam juntos.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use kryptos_protocolo::ErroProtocolo;
use kryptos_protocolo::carta::Carta;
use sha2::{Digest, Sha256};
use vodozemac::olm::{IdentityKeys, OlmMessage};
use zeroize::Zeroizing;

use crate::agenda_gravada;
use crate::caixa::{self, Estado, Indice, Lugar, TAMANHO_PEDIDO};
use crate::cartao::CartaoContato;
use crate::codigo::CodigoContato;
use crate::cofre::{self, Custo, ErroCofre, Resultado, TAMANHO_CHAVE, TAMANHO_SAL, Varredura};
use crate::contatos::{Adicao, Agenda, Contato, ErroAgenda};
use crate::identidade::Identidade;
use crate::qr::{self, GradeQr};
use crate::sessao::{ChaveAvulsa, ErroSessao, Sessao};

/// O que é preciso para regravar o cofre aberto: onde está, a chave que voltou do
/// Keystore e o cabeçalho que não muda (sal e custo). O nonce é novo a cada gravação.
struct Selo {
    pasta: PathBuf,
    chave: Zeroizing<[u8; TAMANHO_CHAVE]>,
    sal: [u8; TAMANHO_SAL],
    custo: Custo,
}

/// Tudo que pertence a uma identidade, sob a mesma trava: a conta, o selo do cofre (se
/// o cofre está aberto), a agenda e as conversas. Trocar de identidade troca tudo de
/// uma vez.
struct Memoria {
    identidade: Identidade,
    selo: Option<Selo>,
    agenda: Agenda,
    /// A caixa selada, só com o cofre aberto.
    caixa: Option<Caixa>,
    /// Uma conversa por contato, a mais recente. Poucos contatos: busca linear.
    conversas: Vec<(CodigoContato, Sessao)>,
}

/// O índice da caixa e o que só vive na memória: a chegada à espera do Keystore, a
/// hora em que cada gaveta foi lida (relógio monotônico) e quantas cartas não abriram
/// nesta execução.
struct Caixa {
    indice: Indice,
    pendente: Option<Pendente>,
    lidas_em: Vec<(u8, Instant)>,
    nao_abertas: u32,
}

impl Caixa {
    fn nova(indice: Indice) -> Self {
        Self {
            indice,
            pendente: None,
            lidas_em: Vec::new(),
            nao_abertas: 0,
        }
    }
}

/// Carta já decifrada, com a gaveta reservada, esperando o HMAC do Keystore.
struct Pendente {
    gaveta: u8,
    nonce: [u8; 24],
    remetente: IdentityKeys,
    texto: Zeroizing<Vec<u8>>,
}

impl Memoria {
    fn nova(identidade: Identidade, selo: Option<Selo>, agenda: Agenda) -> Self {
        Self {
            identidade,
            selo,
            agenda,
            caixa: None,
            conversas: Vec::new(),
        }
    }

    fn contato(&self, codigo: CodigoContato) -> Result<Contato, ErroConversa> {
        self.agenda
            .contatos()
            .iter()
            .find(|c| c.codigo() == codigo)
            .copied()
            .ok_or(ErroConversa::ContatoDesconhecido)
    }

    fn conversa(&mut self, codigo: CodigoContato) -> Option<&mut Sessao> {
        self.conversas
            .iter_mut()
            .find(|(c, _)| *c == codigo)
            .map(|(_, s)| s)
    }

    fn guardar_conversa(&mut self, codigo: CodigoContato, sessao: Sessao) {
        self.conversas.retain(|(c, _)| *c != codigo);
        self.conversas.push((codigo, sessao));
    }

    /// Grava a conta da memória no cofre e prova lendo de volta.
    fn regravar(&self) -> Result<(), ErroConversa> {
        let selo = self.selo.as_ref().ok_or(ErroConversa::CofreFechado)?;
        let arquivo = cofre::selar(&self.identidade, &selo.chave, selo.sal, selo.custo)?;
        cofre::regravar(&selo.pasta, &arquivo)?;
        self.conferir_gravado()
    }

    /// O cofre no disco abre com a chave do selo e traz a mesma conta da memória,
    /// chaves avulsas incluídas.
    fn conferir_gravado(&self) -> Result<(), ErroConversa> {
        let selo = self.selo.as_ref().ok_or(ErroConversa::CofreFechado)?;
        let lida = cofre::abrir(&cofre::ler(&selo.pasta)?, &selo.chave)?;
        match lida.mesma_conta_que(&self.identidade) {
            Ok(true) => Ok(()),
            Ok(false) => Err(ErroConversa::Cofre(ErroCofre::ArquivoCorrompido)),
            Err(e) => {
                eprintln!("Kryptos: conta relida do cofre não virou JSON ({e:?})");
                Err(ErroConversa::Cofre(ErroCofre::ArquivoCorrompido))
            }
        }
    }
}

/// Grava a agenda e prova lendo de volta: o arquivo abre com a mesma chave e traz a
/// mesma agenda, na mesma ordem.
fn gravar_agenda(
    pasta: &Path,
    chave: &[u8; TAMANHO_CHAVE],
    agenda: &Agenda,
    proprio: &vodozemac::olm::IdentityKeys,
) -> Resultado<()> {
    cofre::gravar_agenda(pasta, &agenda_gravada::selar(agenda, chave)?)?;
    let lida = cofre::ler_agenda(pasta)?.ok_or(ErroCofre::AgendaCorrompida)?;
    if agenda_gravada::abrir(&lida, chave, proprio)? != *agenda {
        return Err(ErroCofre::AgendaCorrompida);
    }
    Ok(())
}

/// Grava o índice da caixa e prova lendo de volta.
fn gravar_indice(pasta: &Path, chave: &[u8; TAMANHO_CHAVE], indice: &Indice) -> Resultado<()> {
    caixa::gravar_indice(pasta, &caixa::selar_indice(indice, chave)?)?;
    let lido = caixa::ler_indice(pasta)?.ok_or(ErroCofre::CaixaCorrompida)?;
    if caixa::abrir_indice(&lido, chave)? != *indice {
        return Err(ErroCofre::CaixaCorrompida);
    }
    Ok(())
}

/// Caixa nova: as 100 gavetas vazias (aleatórias) e o índice vazio, tudo conferido.
fn criar_caixa(pasta: &Path, chave: &[u8; TAMANHO_CHAVE]) -> Resultado<Indice> {
    for g in 0..caixa::CAPACIDADE {
        let g = u8::try_from(g).map_err(|_| ErroCofre::CaixaCorrompida)?;
        caixa::gravar_gaveta_conferida(pasta, g, &caixa::gaveta_vazia()?)?;
    }
    let indice = Indice::vazio();
    gravar_indice(pasta, chave, &indice)?;
    Ok(indice)
}

static IDENTIDADE: Mutex<Option<Memoria>> = Mutex::new(None);

fn trava() -> MutexGuard<'static, Option<Memoria>> {
    // Trava envenenada quer dizer que outra thread entrou em pânico segurando-a. O valor
    // guardado continua inteiro, porque toda escrita troca o valor de uma vez. Por isso
    // seguimos com ele em vez de derrubar o app.
    IDENTIDADE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Motivo de uma operação de conversa não acontecer. Não carrega chave nem texto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroConversa {
    Sessao(ErroSessao),
    Cofre(ErroCofre),
    /// Não há identidade na memória.
    SemIdentidade,
    /// Sem cofre aberto não há onde gravar a chave avulsa: ela se perderia ao fechar.
    CofreFechado,
    /// O código não está na agenda: a chave não foi conferida por QR.
    ContatoDesconhecido,
    /// Mensagem comum de um contato sem conversa aberta (o app fechou, por exemplo).
    SemConversa,
    /// Texto acima de `caixa::MAXIMO_TEXTO`.
    TextoGrande,
    /// A carta não pôde ser montada.
    Carta(ErroProtocolo),
}

impl ErroConversa {
    pub fn codigo(&self) -> &'static str {
        match self {
            Self::Sessao(e) => e.codigo(),
            Self::Cofre(e) => e.codigo(),
            Self::SemIdentidade => "SEM_IDENTIDADE",
            Self::CofreFechado => "COFRE_FECHADO",
            Self::ContatoDesconhecido => "CONTATO_DESCONHECIDO",
            Self::SemConversa => "SEM_CONVERSA",
            Self::TextoGrande => "TEXTO_GRANDE",
            Self::Carta(e) => e.codigo(),
        }
    }
}

impl fmt::Display for ErroConversa {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErroConversa {}

impl From<ErroSessao> for ErroConversa {
    fn from(e: ErroSessao) -> Self {
        Self::Sessao(e)
    }
}

impl From<ErroCofre> for ErroConversa {
    fn from(e: ErroCofre) -> Self {
        Self::Cofre(e)
    }
}

/// Gera a identidade do aparelho na primeira chamada e devolve o código de contato.
/// Chamadas seguintes devolvem o mesmo código: tocar duas vezes não troca a chave.
/// Depois de `abrir_cofre`, devolve o código da identidade aberta.
pub fn gerar_identidade() -> CodigoContato {
    let mut guarda = trava();
    guarda
        .get_or_insert_with(|| Memoria::nova(Identidade::nova(), None, Agenda::nova()))
        .identidade
        .codigo_contato()
}

/// Código da identidade na memória, se houver.
pub fn codigo_atual() -> Option<CodigoContato> {
    trava().as_ref().map(|m| m.identidade.codigo_contato())
}

/// Tira a identidade, a chave do cofre, as conversas e a agenda da memória (a conta do
/// vodozemac e a chave do cofre zeram ao sair). É o que acontece quando o app fecha; o
/// teste no aparelho usa para simular a reabertura. A agenda gravada fica no disco e
/// volta com `abrir_cofre`.
pub fn esquecer() {
    *trava() = None;
}

/// Cartão deste aparelho, para desenhar o QR. `None` sem identidade na memória.
pub fn cartao_proprio() -> Option<CartaoContato> {
    trava()
        .as_ref()
        .map(|m| CartaoContato::de_chaves(&m.identidade.chaves_publicas()))
}

/// Põe na agenda o contato do texto lido no QR, conferido por QR. Com o cofre aberto,
/// o disco primeiro: a agenda nova é gravada e lida de volta, e só então troca a da
/// memória. Se a gravação falhar, o contato não entra.
pub fn adicionar_contato_por_qr(texto: &str) -> Result<Adicao, ErroAgenda> {
    let mut guarda = trava();
    let memoria = guarda.as_mut().ok_or(ErroAgenda::SemIdentidade)?;
    let proprio = memoria.identidade.chaves_publicas();
    let mut nova = memoria.agenda.clone();
    let adicao = nova.adicionar_por_qr(texto, &proprio)?;
    if let (Adicao::Novo(_), Some(selo)) = (adicao, memoria.selo.as_ref()) {
        gravar_agenda(&selo.pasta, &selo.chave, &nova, &proprio).map_err(ErroAgenda::Gravacao)?;
    }
    memoria.agenda = nova;
    Ok(adicao)
}

/// Grade do QR deste aparelho, para a tela desenhar. `None` sem identidade na memória.
pub fn grade_propria() -> Result<Option<GradeQr>, qr::ErroQr> {
    cartao_proprio()
        .map(|c| qr::grade_do_cartao(&c))
        .transpose()
}

/// Procura um QR no quadro de brilho da câmera e, se achar, tenta pôr o contato na
/// agenda. `Ok(None)`: o quadro não tem QR legível, e a câmera segue para o próximo.
/// Um QR achado mas recusado (de outro app, adulterado, o próprio) volta como erro.
pub fn adicionar_contato_do_quadro(
    brilho: &[u8],
    largura: usize,
    altura: usize,
    passo: usize,
) -> Result<Option<Adicao>, ErroAgenda> {
    match qr::ler_quadro(brilho, largura, altura, passo)? {
        None => Ok(None),
        Some(texto) => adicionar_contato_por_qr(&texto).map(Some),
    }
}

/// Cópia da agenda, na ordem em que os contatos entraram. Vazia sem identidade.
pub fn contatos() -> Vec<Contato> {
    trava()
        .as_ref()
        .map(|m| m.agenda.contatos().to_vec())
        .unwrap_or_default()
}

/// Sal novo e PIN esticado, para o Kotlin passar no HMAC do Keystore.
pub struct PinPreparado {
    pub sal: [u8; TAMANHO_SAL],
    pub esticado: Zeroizing<[u8; TAMANHO_CHAVE]>,
}

pub fn preparar_pin_novo(pin: &[u8]) -> Resultado<PinPreparado> {
    preparar_pin_novo_com_custo(pin, cofre::CUSTO_PADRAO)
}

fn preparar_pin_novo_com_custo(pin: &[u8], custo: Custo) -> Resultado<PinPreparado> {
    cofre::validar_pin(pin)?;
    let sal = cofre::sal_novo()?;
    let esticado = cofre::esticar_pin(pin, &sal, custo)?;
    Ok(PinPreparado { sal, esticado })
}

/// Grava a agenda da memória e a identidade no cofre novo e guarda o selo para
/// regravar. Recusa se não há identidade ou se o cofre já existe.
///
/// A agenda e a caixa (100 gavetas vazias e o índice) vão primeiro, mesmo vazias:
/// assim os arquivos existem desde a criação e não dizem se há contatos ou mensagens.
/// Se a identidade falhar depois, o que ficou é sobrescrito na próxima tentativa (sem
/// cofre, nada o lê).
pub fn criar_cofre(pasta: &Path, sal: &[u8], chave_do_cofre: &[u8]) -> Resultado<()> {
    criar_cofre_com_custo(pasta, sal, chave_do_cofre, cofre::CUSTO_PADRAO)
}

fn criar_cofre_com_custo(
    pasta: &Path,
    sal: &[u8],
    chave_do_cofre: &[u8],
    custo: Custo,
) -> Resultado<()> {
    let sal: [u8; TAMANHO_SAL] = sal.try_into().map_err(|_| ErroCofre::ArquivoCorrompido)?;
    let chave = cofre::chave_de(chave_do_cofre)?;
    if cofre::existe(pasta) {
        return Err(ErroCofre::JaExiste);
    }
    let mut guarda = trava();
    let memoria = guarda.as_mut().ok_or(ErroCofre::SemIdentidade)?;
    let proprio = memoria.identidade.chaves_publicas();
    gravar_agenda(pasta, &chave, &memoria.agenda, &proprio)?;
    let indice = criar_caixa(pasta, &chave)?;
    let arquivo = cofre::selar(&memoria.identidade, &chave, sal, custo)?;
    cofre::gravar_novo(pasta, &arquivo)?;
    memoria.caixa = Some(Caixa::nova(indice));
    memoria.selo = Some(Selo {
        pasta: pasta.to_path_buf(),
        chave,
        sal,
        custo,
    });
    Ok(())
}

/// Espera que a próxima tentativa vai cumprir antes de conferir o PIN.
pub fn espera_atual(pasta: &Path) -> Resultado<Duration> {
    Ok(cofre::espera(cofre::ler_tentativas(pasta)?))
}

/// Primeira etapa da abertura. Registra a tentativa ANTES de conferir (fechar o app
/// no meio não apaga o erro), cumpre a espera e estica o PIN com o sal do arquivo.
/// PIN fora do formato não conta como tentativa.
pub fn preparar_abertura(pasta: &Path, pin: &[u8]) -> Resultado<Zeroizing<[u8; TAMANHO_CHAVE]>> {
    preparar_abertura_com(pasta, pin, thread::sleep)
}

fn preparar_abertura_com(
    pasta: &Path,
    pin: &[u8],
    dormir: impl Fn(Duration),
) -> Resultado<Zeroizing<[u8; TAMANHO_CHAVE]>> {
    cofre::validar_pin(pin)?;
    let arquivo = cofre::ler(pasta)?;
    let (cabecalho, _) = cofre::Cabecalho::ler(&arquivo)?;
    let falhas = cofre::ler_tentativas(pasta)?;
    cofre::gravar_tentativas(pasta, falhas.saturating_add(1))?;
    let espera = cofre::espera(falhas);
    if !espera.is_zero() {
        dormir(espera);
    }
    cofre::esticar_pin(pin, &cabecalho.sal, cabecalho.custo)
}

/// Segunda etapa: abre o cofre com a chave do Keystore. Deu certo: zera as tentativas,
/// abre a agenda gravada, põe identidade, selo e agenda na memória e devolve o código.
///
/// Agenda ausente (cofre criado antes da agenda gravada): começa vazia e já é gravada.
/// Agenda que não abre: `AgendaCorrompida`, nada vai para a memória e nada é apagado.
/// Caixa: a mesma regra (`CaixaCorrompida`; ausente, criada vazia). As gavetas lidas
/// antes de fechar queimam no `queimar_lidas`, que o app chama logo depois de abrir.
/// O PIN estava certo, então as tentativas já foram zeradas.
pub fn abrir_cofre(pasta: &Path, chave_do_cofre: &[u8]) -> Resultado<CodigoContato> {
    let chave = cofre::chave_de(chave_do_cofre)?;
    let arquivo = cofre::ler(pasta)?;
    let (cabecalho, _) = cofre::Cabecalho::ler(&arquivo)?;
    let identidade = cofre::abrir(&arquivo, &chave)?;
    cofre::gravar_tentativas(pasta, 0)?;
    let proprio = identidade.chaves_publicas();
    let agenda = match cofre::ler_agenda(pasta)? {
        Some(gravada) => agenda_gravada::abrir(&gravada, &chave, &proprio)?,
        None => {
            let vazia = Agenda::nova();
            gravar_agenda(pasta, &chave, &vazia, &proprio)?;
            vazia
        }
    };
    let indice = match caixa::ler_indice(pasta)? {
        Some(gravado) => caixa::abrir_indice(&gravado, &chave)?,
        None => criar_caixa(pasta, &chave)?,
    };
    let codigo = identidade.codigo_contato();
    let selo = Selo {
        pasta: pasta.to_path_buf(),
        chave,
        sal: cabecalho.sal,
        custo: cabecalho.custo,
    };
    let mut memoria = Memoria::nova(identidade, Some(selo), agenda);
    memoria.caixa = Some(Caixa::nova(indice));
    *trava() = Some(memoria);
    Ok(codigo)
}

/// Procura as chaves secretas da identidade em memória (chaves avulsas incluídas) em
/// todos os arquivos da pasta.
pub fn procurar_segredos(pasta: &Path) -> Resultado<Varredura> {
    let segredos = {
        let guarda = trava();
        let memoria = guarda.as_ref().ok_or(ErroCofre::SemIdentidade)?;
        memoria
            .identidade
            .segredos_para_varredura()
            .map_err(|_| ErroCofre::SemIdentidade)?
    };
    cofre::procurar(pasta, &segredos)
}

// ---------------------------------------------------------------- conversas

/// Gera uma chave avulsa assinada, grava a conta no cofre e só então a entrega. Sem
/// cofre aberto, recusa: a chave se perderia ao fechar o app.
pub fn nova_chave_avulsa() -> Result<ChaveAvulsa, ErroConversa> {
    let mut guarda = trava();
    let memoria = guarda.as_mut().ok_or(ErroConversa::SemIdentidade)?;
    if memoria.selo.is_none() {
        return Err(ErroConversa::CofreFechado);
    }
    let avulsa = memoria.identidade.nova_chave_avulsa()?;
    memoria.regravar()?;
    Ok(avulsa)
}

/// Gera `quantas` chaves avulsas, grava a conta no cofre e só então devolve a
/// publicação assinada para o relay. Com 0, só consulta o estoque. `numero` tem de ser
/// maior que o último que o relay aceitou deste aparelho; se não for, o relay responde
/// `NUMERO_VELHO <último>` e a publicação é refeita com o seguinte.
pub fn preparar_publicacao(quantas: usize, numero: u64) -> Result<Vec<u8>, ErroConversa> {
    let mut guarda = trava();
    let memoria = guarda.as_mut().ok_or(ErroConversa::SemIdentidade)?;
    if memoria.selo.is_none() {
        return Err(ErroConversa::CofreFechado);
    }
    let publicacao = memoria.identidade.publicacao(quantas, numero)?;
    memoria.regravar()?;
    Ok(publicacao)
}

/// Há conversa aberta com o contato nesta execução do app?
pub fn tem_conversa(codigo: CodigoContato) -> bool {
    trava()
        .as_ref()
        .is_some_and(|m| m.conversas.iter().any(|(c, _)| *c == codigo))
}

/// Ed25519 do contato da agenda, para pedir a chave avulsa dele no relay. Pública.
pub fn dono_do_contato(codigo: CodigoContato) -> Result<[u8; 32], ErroConversa> {
    let guarda = trava();
    let memoria = guarda.as_ref().ok_or(ErroConversa::SemIdentidade)?;
    Ok(*memoria.contato(codigo)?.chaves().ed25519.as_bytes())
}

/// Busca assinada para o malote do relay: confirma `confirmadas` e pede as próximas
/// cartas. Não precisa do cofre: só assina.
pub fn preparar_busca(
    numero: u64,
    confirmadas: Vec<[u8; kryptos_protocolo::malote::BYTES_ID]>,
) -> Result<Vec<u8>, ErroConversa> {
    let guarda = trava();
    let memoria = guarda.as_ref().ok_or(ErroConversa::SemIdentidade)?;
    Ok(memoria.identidade.busca(numero, confirmadas)?)
}

/// A abre a conversa com um contato da agenda, com a chave avulsa dele. Substitui a
/// conversa anterior com o mesmo contato, se houver.
pub fn abrir_conversa(codigo: CodigoContato, avulsa: &ChaveAvulsa) -> Result<(), ErroConversa> {
    let mut guarda = trava();
    let memoria = guarda.as_mut().ok_or(ErroConversa::SemIdentidade)?;
    let contato = memoria.contato(codigo)?;
    let sessao = Sessao::abrir(&memoria.identidade, &contato, avulsa)?;
    memoria.guardar_conversa(codigo, sessao);
    Ok(())
}

/// Cifra para um contato com conversa aberta. Até `caixa::MAXIMO_TEXTO` bytes.
pub fn cifrar(codigo: CodigoContato, texto: &[u8]) -> Result<OlmMessage, ErroConversa> {
    if texto.len() > caixa::MAXIMO_TEXTO {
        return Err(ErroConversa::TextoGrande);
    }
    let mut guarda = trava();
    let memoria = guarda.as_mut().ok_or(ErroConversa::SemIdentidade)?;
    let sessao = memoria.conversa(codigo).ok_or(ErroConversa::SemConversa)?;
    Ok(sessao.cifrar(texto)?)
}

/// Cifra para um contato com conversa aberta e monta a carta para ele (destinatário =
/// a Ed25519 do contato), pronta para qualquer transporte.
pub fn cifrar_carta(codigo: CodigoContato, texto: &[u8]) -> Result<Vec<u8>, ErroConversa> {
    let destinatario = {
        let guarda = trava();
        let memoria = guarda.as_ref().ok_or(ErroConversa::SemIdentidade)?;
        *memoria.contato(codigo)?.chaves().ed25519.as_bytes()
    };
    let mensagem = cifrar(codigo, texto)?;
    let (tipo, mensagem) = mensagem.to_parts();
    let tipo = if tipo == crate::sessao::TIPO_PRIMEIRA {
        kryptos_protocolo::carta::Tipo::Primeira
    } else {
        kryptos_protocolo::carta::Tipo::Normal
    };
    Carta {
        destinatario,
        tipo,
        mensagem,
    }
    .codificar()
    .map_err(ErroConversa::Carta)
}

/// Recebe uma mensagem de um contato da agenda.
///
/// - Primeira mensagem da conversa que já está aberta (A mandou várias antes de B
///   responder): decifra nela.
/// - Primeira mensagem de uma conversa nova: abre a conversa, grava a conta com a chave
///   avulsa gasta e só então devolve o texto. Se a gravação falhar, a conversa é
///   descartada e o texto não sai: depois de reabrir o app, a mesma mensagem ainda
///   abre, porque o cofre continua com a chave.
/// - Mensagem comum: decifra na conversa aberta.
pub fn receber(
    codigo: CodigoContato,
    mensagem: &OlmMessage,
) -> Result<Zeroizing<Vec<u8>>, ErroConversa> {
    let mut guarda = trava();
    let memoria = guarda.as_mut().ok_or(ErroConversa::SemIdentidade)?;
    let contato = memoria.contato(codigo)?;
    if let OlmMessage::PreKey(primeira) = mensagem {
        let ja_aberta = memoria
            .conversa(codigo)
            .filter(|s| s.identificador() == primeira.session_id());
        if let Some(sessao) = ja_aberta {
            return Ok(sessao.decifrar(mensagem)?);
        }
        if memoria.selo.is_none() {
            return Err(ErroConversa::CofreFechado);
        }
        let (sessao, texto) =
            Sessao::receber_primeira(&mut memoria.identidade, &contato, mensagem)?;
        memoria.regravar()?;
        memoria.guardar_conversa(codigo, sessao);
        return Ok(texto);
    }
    let sessao = memoria.conversa(codigo).ok_or(ErroConversa::SemConversa)?;
    Ok(sessao.decifrar(mensagem)?)
}

// ---------------------------------------------------------------- caixa selada

/// Motivo de uma operação da caixa não acontecer. Não carrega texto, chave nem código.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroCaixa {
    Cofre(ErroCofre),
    /// A carta não está no formato (veio estragada): o app confirma e descarta.
    Carta(ErroProtocolo),
    SemIdentidade,
    CofreFechado,
    /// Já há uma carta esperando o HMAC: termine com `selar_chegada` ou
    /// `desistir_da_chegada` antes de receber outra.
    ChegadaPendente,
    /// `selar_chegada` sem carta esperando.
    SemChegada,
    /// Número de gaveta fora de 0..99.
    GavetaInvalida,
    /// A gaveta não está no estado que a operação pede (ler uma gaveta livre, por exemplo).
    Estado,
}

impl ErroCaixa {
    pub fn codigo(&self) -> &'static str {
        match self {
            Self::Cofre(e) => e.codigo(),
            Self::Carta(e) => e.codigo(),
            Self::SemIdentidade => "SEM_IDENTIDADE",
            Self::CofreFechado => "COFRE_FECHADO",
            Self::ChegadaPendente => "CAIXA_CHEGADA_PENDENTE",
            Self::SemChegada => "CAIXA_SEM_CHEGADA",
            Self::GavetaInvalida => "GAVETA_INVALIDA",
            Self::Estado => "GAVETA_ESTADO",
        }
    }
}

impl fmt::Display for ErroCaixa {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErroCaixa {}

impl From<ErroCofre> for ErroCaixa {
    fn from(e: ErroCofre) -> Self {
        Self::Cofre(e)
    }
}

/// O que fazer com uma carta que chegou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chegada {
    /// Decifrada e com gaveta reservada: pedir ao Keystore o HMAC de `pedido` com a
    /// chave da `gaveta` e chamar `selar_chegada`. Só depois confirmar ao relay.
    Selar {
        gaveta: u8,
        pedido: [u8; TAMANHO_PEDIDO],
    },
    /// A mesma carta já está numa gaveta (o relay entregou de novo): confirmar.
    JaGuardada,
    /// Não abre: conversa encerrada, remetente fora da agenda ou carta adulterada.
    /// Confirmar ao relay; entra na conta de `nao_abertas`.
    NaoAbre,
    /// Para outra pessoa (o rádio ouve todo mundo): ignorar.
    NaoEParaMim,
    /// Sem gaveta livre: não confirmar. A carta fica no relay e a conversa não anda.
    CaixaCheia,
}

/// Uma mensagem na caixa, sem nada do conteúdo: o remetente só aparece ao ler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemDaCaixa {
    pub gaveta: u8,
    /// Ordem de chegada (não é data).
    pub numero: u64,
    pub lida: bool,
}

/// Mensagem aberta para a tela.
pub struct Lida {
    pub remetente: CodigoContato,
    pub texto: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for Lida {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Lida(oculta)")
    }
}

/// Quanto tempo a mensagem lida fica antes de queimar.
pub const TEMPO_ATE_QUEIMAR: Duration = Duration::from_secs(60);

fn com_caixa<T>(
    f: impl FnOnce(&mut Memoria, &Selo, &mut Caixa) -> Result<T, ErroCaixa>,
) -> Result<T, ErroCaixa> {
    let mut guarda = trava();
    let memoria = guarda.as_mut().ok_or(ErroCaixa::SemIdentidade)?;
    let selo = memoria.selo.take().ok_or(ErroCaixa::CofreFechado)?;
    let Some(mut caixa) = memoria.caixa.take() else {
        memoria.selo = Some(selo);
        return Err(ErroCaixa::CofreFechado);
    };
    let r = f(memoria, &selo, &mut caixa);
    memoria.selo = Some(selo);
    memoria.caixa = Some(caixa);
    r
}

fn gaveta_de(numero: u8) -> Result<usize, ErroCaixa> {
    let g = usize::from(numero);
    if g >= caixa::CAPACIDADE {
        return Err(ErroCaixa::GavetaInvalida);
    }
    Ok(g)
}

/// Grava o índice novo e só então troca o da memória.
fn trocar_indice(selo: &Selo, caixa: &mut Caixa, novo: Indice) -> Result<(), ErroCaixa> {
    gravar_indice(&selo.pasta, &selo.chave, &novo)?;
    caixa.indice = novo;
    Ok(())
}

/// Quem mandou e o texto.
type Decifrada = (Contato, Zeroizing<Vec<u8>>);

/// Decifra a mensagem de quem estiver na agenda. `None`: não abre.
/// Primeira mensagem de conversa nova: abre a conversa e grava a conta (chave avulsa
/// gasta) antes de devolver o texto, como em `receber`.
fn decifrar_de_alguem(
    memoria: &mut Memoria,
    selo: &Selo,
    mensagem: &OlmMessage,
) -> Result<Option<Decifrada>, ErroCaixa> {
    if let OlmMessage::PreKey(primeira) = mensagem {
        let quem = primeira.identity_key();
        let Some(contato) = memoria
            .agenda
            .contatos()
            .iter()
            .find(|c| c.chaves().curve25519 == quem)
            .copied()
        else {
            return Ok(None);
        };
        let codigo = contato.codigo();
        let ja_aberta = memoria
            .conversa(codigo)
            .filter(|s| s.identificador() == primeira.session_id());
        if let Some(sessao) = ja_aberta {
            return Ok(sessao.decifrar(mensagem).ok().map(|t| (contato, t)));
        }
        let Ok((sessao, texto)) =
            Sessao::receber_primeira(&mut memoria.identidade, &contato, mensagem)
        else {
            return Ok(None);
        };
        // A conta mudou (chave avulsa gasta): grava e confere antes de o texto seguir.
        let arquivo = cofre::selar(&memoria.identidade, &selo.chave, selo.sal, selo.custo)?;
        cofre::regravar(&selo.pasta, &arquivo)?;
        let lida = cofre::abrir(&cofre::ler(&selo.pasta)?, &selo.chave)?;
        if !matches!(lida.mesma_conta_que(&memoria.identidade), Ok(true)) {
            return Err(ErroCaixa::Cofre(ErroCofre::ArquivoCorrompido));
        }
        memoria.guardar_conversa(codigo, sessao);
        return Ok(Some((contato, texto)));
    }
    // Mensagem comum não diz de quem é: tenta cada conversa aberta. O vodozemac só
    // muda a conversa quando a autenticação da mensagem confere.
    for (codigo, sessao) in &mut memoria.conversas {
        if let Ok(texto) = sessao.decifrar(mensagem) {
            let codigo = *codigo;
            let contato = memoria.contato(codigo).map_err(|_| ErroCaixa::Estado)?;
            return Ok(Some((contato, texto)));
        }
    }
    Ok(None)
}

/// Porta única das cartas, venham do relay ou do modo Direto.
///
/// Ordem: confere o destinatário, reconhece carta repetida e procura gaveta livre
/// ANTES de decifrar. Decifrar anda a conversa, então sem gaveta livre a carta nem é
/// aberta e pode ser entregue de novo depois.
pub fn receber_carta(bytes: &[u8]) -> Result<Chegada, ErroCaixa> {
    let carta = Carta::ler(bytes).map_err(ErroCaixa::Carta)?;
    com_caixa(|memoria, selo, caixa| {
        if caixa.pendente.is_some() {
            return Err(ErroCaixa::ChegadaPendente);
        }
        let eu = memoria.identidade.chaves_publicas();
        if carta.destinatario != *eu.ed25519.as_bytes() {
            return Ok(Chegada::NaoEParaMim);
        }
        let resumo: [u8; 32] = Sha256::digest(bytes).into();
        let ja = caixa.indice.lugares.iter().any(|l| {
            matches!(l.estado, Estado::NaoLida | Estado::Lida | Estado::Reservada)
                && l.resumo == resumo
        });
        if ja {
            return Ok(Chegada::JaGuardada);
        }
        let Some(livre) = caixa
            .indice
            .lugares
            .iter()
            .position(|l| l.estado == Estado::Livre)
        else {
            return Ok(Chegada::CaixaCheia);
        };
        let gaveta = u8::try_from(livre).map_err(|_| ErroCaixa::GavetaInvalida)?;
        let tipo = usize::from(carta.tipo.numero());
        let Ok(mensagem) = crate::sessao::ler_mensagem(tipo, &carta.mensagem) else {
            caixa.nao_abertas = caixa.nao_abertas.saturating_add(1);
            return Ok(Chegada::NaoAbre);
        };
        let Some((contato, texto)) = decifrar_de_alguem(memoria, selo, &mensagem)? else {
            caixa.nao_abertas = caixa.nao_abertas.saturating_add(1);
            return Ok(Chegada::NaoAbre);
        };
        if texto.len() > caixa::MAXIMO_TEXTO {
            caixa.nao_abertas = caixa.nao_abertas.saturating_add(1);
            return Ok(Chegada::NaoAbre);
        }
        // Disco primeiro: a gaveta fica reservada no índice antes de receber a carta.
        let mut novo = caixa.indice.clone();
        novo.lugares[livre] = Lugar {
            estado: Estado::Reservada,
            numero: novo.proximo,
            resumo,
        };
        novo.proximo += 1;
        trocar_indice(selo, caixa, novo)?;
        let nonce: [u8; 24] = cofre::aleatorio()?;
        caixa.pendente = Some(Pendente {
            gaveta,
            nonce,
            remetente: *contato.chaves(),
            texto,
        });
        Ok(Chegada::Selar {
            gaveta,
            pedido: caixa::pedido(gaveta, &nonce),
        })
    })
}

/// Sela a carta que esperava o HMAC, confere lendo de volta e marca como não lida.
/// Devolve a gaveta. Se falhar, a carta continua esperando: tente de novo ou desista.
pub fn selar_chegada(hmac: &[u8]) -> Result<u8, ErroCaixa> {
    com_caixa(|_memoria, selo, caixa| {
        let p = caixa.pendente.take().ok_or(ErroCaixa::SemChegada)?;
        let r = (|| -> Result<(), ErroCaixa> {
            let arquivo = caixa::selar_gaveta(p.gaveta, p.nonce, hmac, &p.remetente, &p.texto)?;
            caixa::gravar_gaveta_conferida(&selo.pasta, p.gaveta, &arquivo)?;
            let (r, t) =
                caixa::abrir_gaveta(p.gaveta, &caixa::ler_gaveta(&selo.pasta, p.gaveta)?, hmac)?;
            if r != p.remetente || *t != *p.texto {
                return Err(ErroCaixa::Cofre(ErroCofre::CaixaCorrompida));
            }
            let mut novo = caixa.indice.clone();
            novo.lugares[usize::from(p.gaveta)].estado = Estado::NaoLida;
            trocar_indice(selo, caixa, novo)
        })();
        match r {
            Ok(()) => Ok(p.gaveta),
            Err(e) => {
                caixa.pendente = Some(p);
                Err(e)
            }
        }
    })
}

/// Desiste da carta que esperava o HMAC (o Keystore falhou). O texto sai da memória;
/// a gaveta reservada queima no próximo `queimar_lidas`.
pub fn desistir_da_chegada() -> Result<(), ErroCaixa> {
    com_caixa(|_m, _s, caixa| caixa.pendente.take().ok_or(ErroCaixa::SemChegada).map(drop))
}

/// Mensagens na caixa (não lidas e lidas ainda não queimadas), na ordem de chegada.
pub fn caixa() -> Result<Vec<ItemDaCaixa>, ErroCaixa> {
    com_caixa(|_m, _s, caixa| {
        let mut itens: Vec<ItemDaCaixa> = caixa
            .indice
            .lugares
            .iter()
            .enumerate()
            .filter(|(_, l)| matches!(l.estado, Estado::NaoLida | Estado::Lida))
            .filter_map(|(g, l)| {
                u8::try_from(g).ok().map(|gaveta| ItemDaCaixa {
                    gaveta,
                    numero: l.numero,
                    lida: l.estado == Estado::Lida,
                })
            })
            .collect();
        itens.sort_by_key(|i| i.numero);
        Ok(itens)
    })
}

/// Cartas que não abriram nesta execução do app.
pub fn nao_abertas() -> Result<u32, ErroCaixa> {
    com_caixa(|_m, _s, caixa| Ok(caixa.nao_abertas))
}

/// Pedido para o HMAC de leitura de uma gaveta não lida ou lida.
pub fn pedido_de_leitura(gaveta: u8) -> Result<[u8; TAMANHO_PEDIDO], ErroCaixa> {
    com_caixa(|_m, selo, caixa| {
        let g = gaveta_de(gaveta)?;
        if !matches!(
            caixa.indice.lugares[g].estado,
            Estado::NaoLida | Estado::Lida
        ) {
            return Err(ErroCaixa::Estado);
        }
        let nonce = caixa::nonce_da_gaveta(&caixa::ler_gaveta(&selo.pasta, gaveta)?)?;
        Ok(caixa::pedido(gaveta, &nonce))
    })
}

/// Abre a mensagem para a tela. Na primeira leitura, marca como lida no disco ANTES
/// de devolver o texto e começa a contar o minuto até a queima.
pub fn ler(gaveta: u8, hmac: &[u8]) -> Result<Lida, ErroCaixa> {
    ler_em(gaveta, hmac, Instant::now())
}

fn ler_em(gaveta: u8, hmac: &[u8], agora: Instant) -> Result<Lida, ErroCaixa> {
    com_caixa(|_m, selo, caixa| {
        let g = gaveta_de(gaveta)?;
        let estado = caixa.indice.lugares[g].estado;
        if !matches!(estado, Estado::NaoLida | Estado::Lida) {
            return Err(ErroCaixa::Estado);
        }
        let (remetente, texto) =
            caixa::abrir_gaveta(gaveta, &caixa::ler_gaveta(&selo.pasta, gaveta)?, hmac)?;
        if estado == Estado::NaoLida {
            let mut novo = caixa.indice.clone();
            novo.lugares[g].estado = Estado::Lida;
            trocar_indice(selo, caixa, novo)?;
            caixa.lidas_em.push((gaveta, agora));
        }
        Ok(Lida {
            remetente: CodigoContato::de_chaves(&remetente),
            texto,
        })
    })
}

/// Queima uma gaveta: marca no índice (disco primeiro), sobrescreve com bytes
/// aleatórios e confere. Falta trocar a chave no Keystore (`chave_trocada`).
fn queimar(selo: &Selo, caixa: &mut Caixa, gaveta: u8) -> Result<(), ErroCaixa> {
    let g = gaveta_de(gaveta)?;
    if caixa.indice.lugares[g].estado != Estado::Queimada {
        let mut novo = caixa.indice.clone();
        novo.lugares[g] = Lugar {
            estado: Estado::Queimada,
            numero: 0,
            resumo: [0; 32],
        };
        trocar_indice(selo, caixa, novo)?;
    }
    caixa::gravar_gaveta_conferida(&selo.pasta, gaveta, &caixa::gaveta_vazia()?)?;
    caixa.lidas_em.retain(|(x, _)| *x != gaveta);
    Ok(())
}

/// Queima as lidas há um minuto ou mais. Devolve as gavetas cuja chave o Kotlin troca.
pub fn queimar_vencidas() -> Result<Vec<u8>, ErroCaixa> {
    queimar_vencidas_em(Instant::now())
}

fn queimar_vencidas_em(agora: Instant) -> Result<Vec<u8>, ErroCaixa> {
    com_caixa(|_m, selo, caixa| {
        let vencidas: Vec<u8> = caixa
            .lidas_em
            .iter()
            .filter(|(_, t)| agora.saturating_duration_since(*t) >= TEMPO_ATE_QUEIMAR)
            .map(|(g, _)| *g)
            .collect();
        for g in &vencidas {
            queimar(selo, caixa, *g)?;
        }
        Ok(vencidas)
    })
}

/// Queima na hora todas as lidas, as reservadas que nunca foram seladas e termina as
/// queimas interrompidas. Chamar ao abrir, ao sair do app e ao apagar a tela.
/// Devolve as gavetas cuja chave o Kotlin troca (inclui as já queimadas que ainda
/// esperam a troca).
pub fn queimar_lidas() -> Result<Vec<u8>, ErroCaixa> {
    com_caixa(|_m, selo, caixa| {
        let alvo: Vec<u8> = caixa
            .indice
            .lugares
            .iter()
            .enumerate()
            .filter(|(g, l)| {
                let esperando = caixa
                    .pendente
                    .as_ref()
                    .is_some_and(|p| usize::from(p.gaveta) == *g);
                matches!(l.estado, Estado::Lida | Estado::Queimada)
                    || (l.estado == Estado::Reservada && !esperando)
            })
            .filter_map(|(g, _)| u8::try_from(g).ok())
            .collect();
        for g in &alvo {
            queimar(selo, caixa, *g)?;
        }
        Ok(alvo)
    })
}

/// O Kotlin trocou a chave da gaveta no Keystore: ela volta a ficar livre.
pub fn chave_trocada(gaveta: u8) -> Result<(), ErroCaixa> {
    com_caixa(|_m, selo, caixa| {
        let g = gaveta_de(gaveta)?;
        match caixa.indice.lugares[g].estado {
            Estado::Livre => Ok(()),
            Estado::Queimada => {
                let mut novo = caixa.indice.clone();
                novo.lugares[g] = Lugar {
                    estado: Estado::Livre,
                    numero: 0,
                    resumo: [0; 32],
                };
                trocar_indice(selo, caixa, novo)
            }
            _ => Err(ErroCaixa::Estado),
        }
    })
}

#[cfg(test)]
mod testes {
    use std::cell::RefCell;
    use std::fs;

    use super::*;
    use crate::contatos::Agenda;

    const BARATO: Custo = Custo {
        memoria_kib: 64,
        passadas: 1,
        paralelismo: 1,
    };

    /// Faz o papel do Keystore: uma função fixa da entrada, fora do núcleo de verdade.
    fn keystore_falso(esticado: &[u8]) -> Vec<u8> {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(b"keystore falso");
        h.update(esticado);
        h.finalize().to_vec()
    }

    /// Os testes que tocam o estado global (identidade e agenda) pegam esta trava,
    /// para não disputarem entre si quando o `cargo test` roda em paralelo.
    static SERIE: Mutex<()> = Mutex::new(());

    fn em_serie() -> MutexGuard<'static, ()> {
        SERIE.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[test]
    fn ciclo_completo_criar_esquecer_errar_abrir() -> Result<(), Box<dyn std::error::Error>> {
        let _serie = em_serie();
        let pasta = std::env::temp_dir().join(format!("kryptos-aparelho-{}", std::process::id()));
        let _ = fs::remove_dir_all(&pasta);
        fs::create_dir_all(&pasta)?;
        let r = (|| -> Result<(), Box<dyn std::error::Error>> {
            // gerar duas vezes devolve o mesmo código
            let codigo = gerar_identidade();
            assert_eq!(gerar_identidade(), codigo);

            // criar
            let novo = preparar_pin_novo_com_custo(b"135790", BARATO)?;
            let chave = keystore_falso(novo.esticado.as_ref());
            criar_cofre_com_custo(&pasta, &novo.sal, &chave, BARATO)?;
            assert_eq!(
                criar_cofre_com_custo(&pasta, &novo.sal, &chave, BARATO).err(),
                Some(ErroCofre::JaExiste)
            );

            // "fechar o app"
            esquecer();
            assert_eq!(codigo_atual(), None);

            // PIN errado: não abre, conta tentativa, memória continua vazia
            let dormiu = RefCell::new(Vec::new());
            let dormir = |d: Duration| dormiu.borrow_mut().push(d);
            let e = preparar_abertura_com(&pasta, b"000000", dormir)?;
            assert_eq!(
                abrir_cofre(&pasta, &keystore_falso(e.as_ref())).err(),
                Some(ErroCofre::PinIncorreto)
            );
            assert_eq!(codigo_atual(), None);
            assert_eq!(cofre::ler_tentativas(&pasta)?, 1);

            // PIN fora do formato não conta
            assert_eq!(
                preparar_abertura_com(&pasta, b"12", dormir).err(),
                Some(ErroCofre::PinInvalido)
            );
            assert_eq!(cofre::ler_tentativas(&pasta)?, 1);

            // mais quatro erros: a sexta tentativa já espera 30 s
            for _ in 0..4 {
                let e = preparar_abertura_com(&pasta, b"000000", dormir)?;
                assert!(abrir_cofre(&pasta, &keystore_falso(e.as_ref())).is_err());
            }
            assert!(dormiu.borrow().is_empty(), "esperou antes da hora");
            assert_eq!(espera_atual(&pasta)?, Duration::from_secs(30));

            // PIN certo: espera, abre, mesmo código, tentativas zeradas
            let e = preparar_abertura_com(&pasta, b"135790", dormir)?;
            assert_eq!(dormiu.borrow().as_slice(), &[Duration::from_secs(30)]);
            assert_eq!(abrir_cofre(&pasta, &keystore_falso(e.as_ref()))?, codigo);
            assert_eq!(codigo_atual(), Some(codigo));
            assert_eq!(gerar_identidade(), codigo);
            assert_eq!(cofre::ler_tentativas(&pasta)?, 0);

            // chave do Keystore errada (aparelho trocado): não abre
            esquecer();
            let e = preparar_abertura_com(&pasta, b"135790", dormir)?;
            let mut outra = keystore_falso(e.as_ref());
            outra[0] ^= 1;
            assert_eq!(
                abrir_cofre(&pasta, &outra).err(),
                Some(ErroCofre::PinIncorreto)
            );

            // a varredura não acha nada no disco, e precisa de identidade na memória
            assert_eq!(
                procurar_segredos(&pasta).err(),
                Some(ErroCofre::SemIdentidade)
            );
            let e = preparar_abertura_com(&pasta, b"135790", dormir)?;
            abrir_cofre(&pasta, &keystore_falso(e.as_ref()))?;
            let v = procurar_segredos(&pasta)?;
            assert_eq!((v.arquivos, v.ocorrencias, v.ilegiveis), (104, 0, 0));

            // a agenda volta ao abrir o cofre de novo
            let outro = Identidade::nova().chaves_publicas();
            adicionar_contato_por_qr(&CartaoContato::de_chaves(&outro).como_texto())?;
            assert_eq!(contatos().len(), 1);
            let antes = contatos();
            esquecer();
            assert!(contatos().is_empty());
            let e = preparar_abertura_com(&pasta, b"135790", dormir)?;
            abrir_cofre(&pasta, &keystore_falso(e.as_ref()))?;
            assert_eq!(contatos(), antes, "agenda não voltou ao abrir");
            esquecer();
            Ok(())
        })();
        fs::remove_dir_all(&pasta)?;
        r
    }

    #[test]
    fn agenda_segue_a_identidade_da_memoria() -> Result<(), ErroAgenda> {
        let _serie = em_serie();
        esquecer();
        let outro = Identidade::nova();
        let texto = CartaoContato::de_chaves(&outro.chaves_publicas()).como_texto();

        // sem identidade: não há QR próprio e nada entra
        assert!(cartao_proprio().is_none());
        assert_eq!(
            adicionar_contato_por_qr(&texto),
            Err(ErroAgenda::SemIdentidade)
        );
        assert!(contatos().is_empty());

        // com identidade: o QR próprio é o da identidade e é recusado como contato
        let meu_codigo = gerar_identidade();
        let meu = cartao_proprio().ok_or(ErroAgenda::SemIdentidade)?;
        assert_eq!(meu.codigo(), meu_codigo);
        assert_eq!(
            adicionar_contato_por_qr(&meu.como_texto()),
            Err(ErroAgenda::Proprio)
        );

        // o QR do outro entra uma vez só
        assert_eq!(
            adicionar_contato_por_qr(&texto)?,
            Adicao::Novo(outro.codigo_contato())
        );
        assert_eq!(
            adicionar_contato_por_qr(&texto)?,
            Adicao::JaEstava(outro.codigo_contato())
        );
        assert_eq!(contatos().len(), 1);

        // pelo quadro da câmera: sem QR não muda nada; com o QR de outro, entra
        let outro2 = Identidade::nova();
        let grade = qr::grade_do_cartao(&CartaoContato::de_chaves(&outro2.chaves_publicas()))?;
        let (lado, foto) = qr::testes::Foto::nitida(4).desenhar(&grade);
        assert_eq!(
            adicionar_contato_do_quadro(&vec![200u8; lado * lado], lado, lado, lado)?,
            None
        );
        assert_eq!(
            adicionar_contato_do_quadro(&foto, lado, lado, lado)?,
            Some(Adicao::Novo(outro2.codigo_contato()))
        );
        assert_eq!(contatos().len(), 2);
        // o próprio QR pela câmera é recusado
        let (lado, minha_foto) = qr::testes::Foto::nitida(4)
            .desenhar(&grade_propria()?.ok_or(ErroAgenda::SemIdentidade)?);
        assert_eq!(
            adicionar_contato_do_quadro(&minha_foto, lado, lado, lado),
            Err(ErroAgenda::Proprio)
        );
        // quadro de tamanho errado não derruba (lado e quadro do mesmo desenho: o QR de
        // cerca de 1 em 100 cartões sai na versão 6, com outro lado)
        assert_eq!(
            adicionar_contato_do_quadro(&minha_foto, lado + 1, lado, lado + 1),
            Err(ErroAgenda::Quadro(qr::ErroQr::QuadroInvalido))
        );

        // "fechar o app" apaga a agenda junto com a identidade
        esquecer();
        assert!(contatos().is_empty());
        assert!(cartao_proprio().is_none());
        assert_eq!(grade_propria(), Ok(None));
        Ok(())
    }

    #[test]
    fn sem_identidade_nao_cria_cofre_e_abrir_sem_cofre_avisa() -> Result<(), std::io::Error> {
        let pasta = std::env::temp_dir().join(format!("kryptos-vazio-{}", std::process::id()));
        let _ = fs::remove_dir_all(&pasta);
        fs::create_dir_all(&pasta)?;
        assert_eq!(
            preparar_abertura_com(&pasta, b"135790", |_| {}).err(),
            Some(ErroCofre::NaoExiste)
        );
        assert_eq!(
            abrir_cofre(&pasta, &[0u8; 32]).err(),
            Some(ErroCofre::NaoExiste)
        );
        assert_eq!(
            criar_cofre_com_custo(&pasta, &[0u8; 15], &[0u8; 32], BARATO).err(),
            Some(ErroCofre::ArquivoCorrompido),
            "sal de tamanho errado"
        );
        fs::remove_dir_all(&pasta)?;
        Ok(())
    }

    // ---------- conversas e chaves avulsas no cofre ----------

    const PIN: &[u8] = b"246801";

    fn pasta_de_teste(nome: &str) -> Result<PathBuf, std::io::Error> {
        let pasta = std::env::temp_dir().join(format!("kryptos-{nome}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&pasta);
        fs::create_dir_all(&pasta)?;
        Ok(pasta)
    }

    /// B: identidade nova na memória e cofre criado com custo baixo.
    fn b_com_cofre(pasta: &Path) -> Result<CodigoContato, Box<dyn std::error::Error>> {
        esquecer();
        let codigo = gerar_identidade();
        let novo = preparar_pin_novo_com_custo(PIN, BARATO)?;
        criar_cofre_com_custo(
            pasta,
            &novo.sal,
            &keystore_falso(novo.esticado.as_ref()),
            BARATO,
        )?;
        Ok(codigo)
    }

    /// "Fecha e abre o app": tira tudo da memória e abre o cofre de novo. Devolve o
    /// código aberto. A agenda volta do disco; as conversas, não.
    fn reabrir(pasta: &Path) -> Result<CodigoContato, Box<dyn std::error::Error>> {
        esquecer();
        let e = preparar_abertura_com(pasta, PIN, |_| {})?;
        Ok(abrir_cofre(pasta, &keystore_falso(e.as_ref()))?)
    }

    /// A, fora do aparelho global: identidade própria, B na agenda dela pelo QR de B.
    fn a_que_leu_o_qr_de_b() -> Result<(Identidade, Contato), Box<dyn std::error::Error>> {
        let a = Identidade::nova();
        let mut agenda = Agenda::nova();
        let qr_de_b = cartao_proprio().ok_or("sem identidade")?.como_texto();
        agenda.adicionar_por_qr(&qr_de_b, &a.chaves_publicas())?;
        Ok((a, agenda.contatos()[0]))
    }

    /// B lê o QR de A.
    fn b_le_o_qr_de(a: &Identidade) -> Result<CodigoContato, Box<dyn std::error::Error>> {
        adicionar_contato_por_qr(&CartaoContato::de_chaves(&a.chaves_publicas()).como_texto())?;
        Ok(a.codigo_contato())
    }

    fn com_pasta(
        nome: &str,
        teste: impl FnOnce(&Path) -> Result<(), Box<dyn std::error::Error>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let _serie = em_serie();
        let pasta = pasta_de_teste(nome)?;
        let r = teste(&pasta);
        esquecer();
        fs::remove_dir_all(&pasta)?;
        r
    }

    #[test]
    fn chave_avulsa_sobrevive_a_fechar_e_a_gasta_continua_gasta()
    -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("avulsa", |pasta| {
            let codigo_b = b_com_cofre(pasta)?;
            let avulsa = nova_chave_avulsa()?;
            let (a, b_para_a) = a_que_leu_o_qr_de_b()?;
            let mut sa = Sessao::abrir(&a, &b_para_a, &avulsa)?;
            let m1 = sa.cifrar(b"um")?;
            let m2 = sa.cifrar(b"dois")?;

            // B fecha o app antes de a mensagem chegar.
            assert_eq!(reabrir(pasta)?, codigo_b);
            let codigo_a = b_le_o_qr_de(&a)?;
            assert_eq!(receber(codigo_a, &m1)?.as_slice(), b"um");
            assert_eq!(
                receber(codigo_a, &m2)?.as_slice(),
                b"dois",
                "segunda primeira"
            );
            assert_eq!(
                receber(codigo_a, &m1).err(),
                Some(ErroConversa::Sessao(ErroSessao::JaUsada)),
                "repetida com a conversa aberta"
            );
            let resposta = cifrar(codigo_a, b"oi, A")?;
            assert_eq!(sa.decifrar(&resposta)?.as_slice(), b"oi, A");
            let m3 = sa.cifrar(b"tres")?;
            assert_eq!(receber(codigo_a, &m3)?.as_slice(), b"tres");

            // Fecha de novo: a chave avulsa gasta continua gasta, e a primeira mensagem
            // repetida não abre outra conversa (nem mostra o texto de novo).
            reabrir(pasta)?;
            let codigo_a = b_le_o_qr_de(&a)?;
            assert_eq!(
                receber(codigo_a, &m1).err(),
                Some(ErroConversa::Sessao(ErroSessao::ChaveAvulsaDesconhecida))
            );
            // A conversa não sobrevive a fechar o app (fica só em memória).
            let m4 = sa.cifrar(b"quatro")?;
            assert_eq!(
                receber(codigo_a, &m4).err(),
                Some(ErroConversa::SemConversa)
            );
            Ok(())
        })
    }

    /// Varredura calibrada (regra 5.3): com chaves avulsas na conta, o cofre não tem
    /// nenhum segredo em claro; a mesma conta gravada sem cifra é achada.
    #[test]
    fn varredura_nao_acha_chave_avulsa_no_cofre() -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("varredura-avulsa", |pasta| {
            b_com_cofre(pasta)?;
            for _ in 0..3 {
                nova_chave_avulsa()?;
            }
            // cofre, tentativas, agenda, índice da caixa e 100 gavetas
            let v = procurar_segredos(pasta)?;
            assert_eq!((v.arquivos, v.ocorrencias, v.ilegiveis), (104, 0, 0));
            let em_claro = trava()
                .as_ref()
                .ok_or("sem identidade")?
                .identidade
                .pickle_em_claro_para_teste()?;
            fs::write(pasta.join("vazamento.json"), em_claro.as_slice())?;
            let v = procurar_segredos(pasta)?;
            assert!(v.ocorrencias >= 5, "achou {} de 5 segredos", v.ocorrencias);
            Ok(())
        })
    }

    #[test]
    fn sem_cofre_nao_entrega_nem_gasta_chave_avulsa() -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("sem-cofre", |pasta| {
            esquecer();
            assert_eq!(nova_chave_avulsa().err(), Some(ErroConversa::SemIdentidade));
            gerar_identidade();
            assert_eq!(nova_chave_avulsa().err(), Some(ErroConversa::CofreFechado));
            // Sem cofre, B também não gasta chave avulsa ao receber: não teria onde
            // gravar que gastou. A chave é gerada por fora da regra só para o teste.
            let avulsa = trava()
                .as_mut()
                .ok_or("sem identidade")?
                .identidade
                .nova_chave_avulsa()?;
            let (a, b_para_a) = a_que_leu_o_qr_de_b()?;
            let m = Sessao::abrir(&a, &b_para_a, &avulsa)?.cifrar(b"x")?;
            let codigo_a = b_le_o_qr_de(&a)?;
            assert_eq!(
                receber(codigo_a, &m).err(),
                Some(ErroConversa::CofreFechado)
            );
            // A recusa não gastou a chave: com o cofre criado, a mesma mensagem abre.
            let novo = preparar_pin_novo_com_custo(PIN, BARATO)?;
            criar_cofre_com_custo(
                pasta,
                &novo.sal,
                &keystore_falso(novo.esticado.as_ref()),
                BARATO,
            )?;
            assert_eq!(receber(codigo_a, &m)?.as_slice(), b"x");
            cofre::apagar(pasta)?;

            // Cofre apagado com o app aberto: não entrega chave nova.
            b_com_cofre(pasta)?;
            let avulsa = nova_chave_avulsa()?;
            let (a, b_para_a) = a_que_leu_o_qr_de_b()?;
            let m1 = Sessao::abrir(&a, &b_para_a, &avulsa)?.cifrar(b"um")?;
            let codigo_a = b_le_o_qr_de(&a)?;
            let arquivo = cofre::ler(pasta)?;
            cofre::apagar(pasta)?;
            assert_eq!(
                nova_chave_avulsa().err(),
                Some(ErroConversa::Cofre(ErroCofre::NaoExiste))
            );
            // E a primeira mensagem não sai sem a chave gasta gravada.
            assert_eq!(
                receber(codigo_a, &m1).err(),
                Some(ErroConversa::Cofre(ErroCofre::NaoExiste))
            );
            // O cofre de antes volta (como depois de uma falha passageira de disco):
            // reaberto, a mesma mensagem abre, porque a chave não foi gravada como gasta.
            cofre::gravar_novo(pasta, &arquivo)?;
            reabrir(pasta)?;
            let codigo_a = b_le_o_qr_de(&a)?;
            assert_eq!(receber(codigo_a, &m1)?.as_slice(), b"um");
            Ok(())
        })
    }

    #[test]
    fn contato_fora_da_agenda_e_conversa_fechada_sao_recusados()
    -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("fora-da-agenda", |pasta| {
            b_com_cofre(pasta)?;
            let avulsa = nova_chave_avulsa()?;
            let (a, b_para_a) = a_que_leu_o_qr_de_b()?;
            let m1 = Sessao::abrir(&a, &b_para_a, &avulsa)?.cifrar(b"um")?;
            let estranho = Identidade::nova().codigo_contato();
            assert_eq!(
                receber(a.codigo_contato(), &m1).err(),
                Some(ErroConversa::ContatoDesconhecido),
                "A ainda não está na agenda de B"
            );
            assert_eq!(
                abrir_conversa(estranho, &avulsa).err(),
                Some(ErroConversa::ContatoDesconhecido)
            );
            let codigo_a = b_le_o_qr_de(&a)?;
            assert_eq!(
                cifrar(codigo_a, b"x").err(),
                Some(ErroConversa::SemConversa)
            );
            // A chave não foi gasta pela recusa: a mesma mensagem abre agora.
            assert_eq!(receber(codigo_a, &m1)?.as_slice(), b"um");
            Ok(())
        })
    }

    /// B abre a conversa (B escreve primeiro para A) pelo aparelho.
    #[test]
    fn b_tambem_abre_conversa_pelo_aparelho() -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("abrir-conversa", |pasta| {
            b_com_cofre(pasta)?;
            let (mut a, a_ve_b) = a_que_leu_o_qr_de_b()?;
            let codigo_a = b_le_o_qr_de(&a)?;
            let avulsa_de_a = a.nova_chave_avulsa()?;
            abrir_conversa(codigo_a, &avulsa_de_a)?;
            let m = cifrar(codigo_a, b"de B")?;
            let (_, lido) = Sessao::receber_primeira(&mut a, &a_ve_b, &m)?;
            assert_eq!(lido.as_slice(), b"de B");
            Ok(())
        })
    }

    /// Calibração da releitura (regra 5.3): o cofre com a conta da memória confere; com
    /// uma chave avulsa a menos, ou com outra conta aberta pela mesma chave, não.
    #[test]
    fn releitura_recusa_cofre_com_outra_conta() -> Result<(), Box<dyn std::error::Error>> {
        let pasta = pasta_de_teste("releitura")?;
        let r = (|| -> Result<(), Box<dyn std::error::Error>> {
            let chave = cofre::chave_de(&[5u8; 32])?;
            let sal = cofre::sal_novo()?;
            let mut m = Memoria::nova(Identidade::nova(), None, Agenda::nova());
            cofre::gravar_novo(&pasta, &cofre::selar(&m.identidade, &chave, sal, BARATO)?)?;
            m.selo = Some(Selo {
                pasta: pasta.clone(),
                chave: cofre::chave_de(&[5u8; 32])?,
                sal,
                custo: BARATO,
            });
            m.conferir_gravado()?;
            let antes = cofre::ler(&pasta)?;
            // Uma chave avulsa a mais na memória do que no disco.
            m.identidade.nova_chave_avulsa()?;
            assert_eq!(
                m.conferir_gravado().err(),
                Some(ErroConversa::Cofre(ErroCofre::ArquivoCorrompido))
            );
            m.regravar()?;
            m.conferir_gravado()?;
            // Volta o arquivo de antes: falta a chave avulsa.
            cofre::regravar(&pasta, &antes)?;
            assert!(m.conferir_gravado().is_err());
            // Outra conta, cifrada com a mesma chave.
            let outra = Identidade::nova();
            cofre::regravar(&pasta, &cofre::selar(&outra, &chave, sal, BARATO)?)?;
            assert_eq!(
                m.conferir_gravado().err(),
                Some(ErroConversa::Cofre(ErroCofre::ArquivoCorrompido))
            );
            Ok(())
        })();
        fs::remove_dir_all(&pasta)?;
        r
    }

    /// As chaves da publicação sobrevivem a fechar o app antes de alguém retirá-las.
    #[test]
    fn chaves_publicadas_sobrevivem_a_fechar() -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("publicacao", |pasta| {
            esquecer();
            gerar_identidade();
            assert_eq!(
                preparar_publicacao(1, 1).err(),
                Some(ErroConversa::CofreFechado)
            );
            b_com_cofre(pasta)?;
            let bytes = preparar_publicacao(2, 1)?;
            let publicada = kryptos_protocolo::Publicacao::conferir(&bytes)?;
            assert_eq!(publicada.chaves.len(), 2);
            reabrir(pasta)?;
            let (a, b_para_a) = a_que_leu_o_qr_de_b()?;
            let codigo_a = b_le_o_qr_de(&a)?;
            for (i, k) in publicada.chaves.iter().enumerate() {
                let avulsa = ChaveAvulsa::da_retirada(&k.para_bytes())?;
                let texto = format!("mensagem {i}");
                let m = Sessao::abrir(&a, &b_para_a, &avulsa)?.cifrar(texto.as_bytes())?;
                assert_eq!(receber(codigo_a, &m)?.as_slice(), texto.as_bytes());
            }
            Ok(())
        })
    }

    #[test]
    fn codigos_de_erro_da_conversa_sao_distintos() {
        let todos = [
            ErroConversa::SemIdentidade,
            ErroConversa::CofreFechado,
            ErroConversa::ContatoDesconhecido,
            ErroConversa::SemConversa,
            ErroConversa::Sessao(ErroSessao::JaUsada),
            ErroConversa::Cofre(ErroCofre::NaoExiste),
        ]
        .map(|e| e.codigo());
        let mut unicos = todos.to_vec();
        unicos.sort_unstable();
        unicos.dedup();
        assert_eq!(unicos.len(), todos.len());
    }

    // ---------- agenda gravada ----------

    fn texto_de_outro() -> (CodigoContato, String) {
        let outro = Identidade::nova();
        (
            outro.codigo_contato(),
            CartaoContato::de_chaves(&outro.chaves_publicas()).como_texto(),
        )
    }

    fn bytes_da_agenda(pasta: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        Ok(cofre::ler_agenda(pasta)?.ok_or("sem agenda no disco")?)
    }

    #[test]
    fn agenda_gravada_volta_depois_de_fechar_e_some_com_apagar()
    -> Result<(), Box<dyn std::error::Error>> {
        let _serie = em_serie();
        let pasta = pasta_de_teste("agenda-volta")?;
        let r = (|| -> Result<(), Box<dyn std::error::Error>> {
            // contato lido antes de o cofre existir vai junto na criação
            esquecer();
            gerar_identidade();
            let (primeiro, texto1) = texto_de_outro();
            assert_eq!(adicionar_contato_por_qr(&texto1)?, Adicao::Novo(primeiro));
            assert!(
                cofre::ler_agenda(&pasta)?.is_none(),
                "gravou antes do cofre"
            );
            let novo = preparar_pin_novo_com_custo(PIN, BARATO)?;
            criar_cofre_com_custo(
                &pasta,
                &novo.sal,
                &keystore_falso(novo.esticado.as_ref()),
                BARATO,
            )?;
            let na_criacao = bytes_da_agenda(&pasta)?;
            assert_eq!(na_criacao.len(), agenda_gravada::TAMANHO_ARQUIVO);

            // contato novo com o cofre aberto: arquivo regravado, mesmo tamanho
            let (segundo, texto2) = texto_de_outro();
            assert_eq!(adicionar_contato_por_qr(&texto2)?, Adicao::Novo(segundo));
            let depois = bytes_da_agenda(&pasta)?;
            assert_ne!(depois, na_criacao);
            assert_eq!(depois.len(), agenda_gravada::TAMANHO_ARQUIVO);

            // o mesmo QR de novo não regrava
            assert_eq!(
                adicionar_contato_por_qr(&texto2)?,
                Adicao::JaEstava(segundo)
            );
            assert_eq!(bytes_da_agenda(&pasta)?, depois);

            // fecha e abre: os dois voltam, na mesma ordem
            let antes = contatos();
            reabrir(&pasta)?;
            assert_eq!(contatos(), antes);
            assert_eq!(
                contatos().iter().map(|c| c.codigo()).collect::<Vec<_>>(),
                vec![primeiro, segundo]
            );

            // apagar leva a agenda junto
            esquecer();
            cofre::apagar(&pasta)?;
            assert!(cofre::ler_agenda(&pasta)?.is_none());
            assert_eq!(fs::read_dir(&pasta)?.count(), 0);
            Ok(())
        })();
        esquecer();
        fs::remove_dir_all(&pasta)?;
        r
    }

    #[test]
    fn cofre_sem_agenda_abre_com_agenda_vazia_ja_gravada() -> Result<(), Box<dyn std::error::Error>>
    {
        let _serie = em_serie();
        let pasta = pasta_de_teste("agenda-ausente")?;
        let r = (|| -> Result<(), Box<dyn std::error::Error>> {
            b_com_cofre(&pasta)?;
            // cofre criado antes da agenda gravada existir
            fs::remove_file(pasta.join(cofre::NOME_AGENDA))?;
            reabrir(&pasta)?;
            assert!(contatos().is_empty());
            assert_eq!(
                bytes_da_agenda(&pasta)?.len(),
                agenda_gravada::TAMANHO_ARQUIVO
            );
            Ok(())
        })();
        esquecer();
        fs::remove_dir_all(&pasta)?;
        r
    }

    #[test]
    fn agenda_adulterada_nao_abre_e_nada_e_apagado() -> Result<(), Box<dyn std::error::Error>> {
        let _serie = em_serie();
        let pasta = pasta_de_teste("agenda-adulterada")?;
        let r = (|| -> Result<(), Box<dyn std::error::Error>> {
            b_com_cofre(&pasta)?;
            adicionar_contato_por_qr(&texto_de_outro().1)?;
            let mut agenda = bytes_da_agenda(&pasta)?;
            let meio = agenda.len() / 2;
            agenda[meio] ^= 0x80;
            fs::write(pasta.join(cofre::NOME_AGENDA), &agenda)?;
            let cofre_antes = cofre::ler(&pasta)?;

            esquecer();
            let e = preparar_abertura_com(&pasta, PIN, |_| {})?;
            assert_eq!(
                abrir_cofre(&pasta, &keystore_falso(e.as_ref())).err(),
                Some(ErroCofre::AgendaCorrompida)
            );
            assert_eq!(codigo_atual(), None, "abriu pela metade");
            assert!(contatos().is_empty());
            assert_eq!(bytes_da_agenda(&pasta)?, agenda, "mexeu na agenda");
            assert_eq!(cofre::ler(&pasta)?, cofre_antes, "mexeu no cofre");
            // o PIN estava certo: não fica contando como erro
            assert_eq!(cofre::ler_tentativas(&pasta)?, 0);
            Ok(())
        })();
        esquecer();
        fs::remove_dir_all(&pasta)?;
        r
    }

    #[test]
    fn gravacao_da_agenda_falha_e_o_contato_nao_entra() -> Result<(), Box<dyn std::error::Error>> {
        let _serie = em_serie();
        let pasta = pasta_de_teste("agenda-falha")?;
        let r = (|| -> Result<(), Box<dyn std::error::Error>> {
            b_com_cofre(&pasta)?;
            let gravada = bytes_da_agenda(&pasta)?;
            // uma pasta no lugar do arquivo: a troca de nome falha
            fs::remove_file(pasta.join(cofre::NOME_AGENDA))?;
            fs::create_dir(pasta.join(cofre::NOME_AGENDA))?;
            let (codigo, texto) = texto_de_outro();
            assert!(matches!(
                adicionar_contato_por_qr(&texto),
                Err(ErroAgenda::Gravacao(ErroCofre::Disco(_)))
            ));
            assert!(contatos().is_empty(), "entrou sem gravar");
            assert!(
                !pasta.join(format!("{}.novo", cofre::NOME_AGENDA)).exists(),
                "temporário ficou no disco"
            );
            // disco de volta: o mesmo QR entra e grava
            fs::remove_dir(pasta.join(cofre::NOME_AGENDA))?;
            fs::write(pasta.join(cofre::NOME_AGENDA), &gravada)?;
            assert_eq!(adicionar_contato_por_qr(&texto)?, Adicao::Novo(codigo));
            reabrir(&pasta)?;
            assert_eq!(
                contatos().iter().map(|c| c.codigo()).collect::<Vec<_>>(),
                vec![codigo]
            );
            Ok(())
        })();
        esquecer();
        fs::remove_dir_all(&pasta)?;
        r
    }

    // ---------- caixa selada ----------

    /// Faz o papel das 100 chaves de gaveta do Keystore: uma chave por gaveta, que
    /// pode ser trocada. O HMAC é trocado por SHA-256(chave || pedido): basta que seja
    /// uma função fixa da chave e do pedido.
    struct KeystoreDasGavetas {
        chaves: Vec<[u8; 32]>,
    }

    impl KeystoreDasGavetas {
        fn novo() -> Self {
            Self {
                chaves: (0..caixa::CAPACIDADE)
                    .map(|i| Sha256::digest(format!("gaveta {i}").as_bytes()).into())
                    .collect(),
            }
        }

        fn hmac(&self, gaveta: u8, pedido: &[u8]) -> Vec<u8> {
            let mut h = Sha256::new();
            h.update(self.chaves[usize::from(gaveta)]);
            h.update(pedido);
            h.finalize().to_vec()
        }

        fn trocar(&mut self, gaveta: u8) {
            let velha = self.chaves[usize::from(gaveta)];
            self.chaves[usize::from(gaveta)] = Sha256::digest(velha).into();
        }
    }

    /// A conversa de A para B (B no aparelho global, com cofre): A leu o QR de B, B leu
    /// o de A, A abriu a conversa com uma chave avulsa de B.
    fn a_conversa_com_b() -> Result<(Identidade, Sessao), Box<dyn std::error::Error>> {
        let avulsa = nova_chave_avulsa()?;
        let (a, b_para_a) = a_que_leu_o_qr_de_b()?;
        b_le_o_qr_de(&a)?;
        let sessao = Sessao::abrir(&a, &b_para_a, &avulsa)?;
        Ok((a, sessao))
    }

    fn carta_para_b(m: &OlmMessage) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        use kryptos_protocolo::carta::Tipo;
        let destinatario = *cartao_proprio()
            .ok_or("sem identidade")?
            .chaves()
            .ed25519
            .as_bytes();
        let (tipo, mensagem) = m.to_parts();
        let tipo = if tipo == crate::sessao::TIPO_PRIMEIRA {
            Tipo::Primeira
        } else {
            Tipo::Normal
        };
        Ok(Carta {
            destinatario,
            tipo,
            mensagem,
        }
        .codificar()?)
    }

    /// Recebe e sela pelo caminho do app. Devolve a gaveta.
    fn chegar(ks: &KeystoreDasGavetas, carta: &[u8]) -> Result<u8, Box<dyn std::error::Error>> {
        match receber_carta(carta)? {
            Chegada::Selar { gaveta, pedido } => Ok(selar_chegada(&ks.hmac(gaveta, &pedido))?),
            outra => Err(format!("esperava Selar, veio {outra:?}").into()),
        }
    }

    fn ler_pelo_app(
        ks: &KeystoreDasGavetas,
        gaveta: u8,
        agora: Instant,
    ) -> Result<Lida, Box<dyn std::error::Error>> {
        let pedido = pedido_de_leitura(gaveta)?;
        Ok(ler_em(gaveta, &ks.hmac(gaveta, &pedido), agora)?)
    }

    /// Quantas vezes o texto aparece nos arquivos da pasta, em qualquer das formas da
    /// varredura (bytes, hex, base64, lista JSON).
    fn achados_no_disco(pasta: &Path, texto: &[u8]) -> Result<u32, ErroCofre> {
        Ok(cofre::procurar(pasta, &[Zeroizing::new(texto.to_vec())])?.ocorrencias)
    }

    #[test]
    fn caixa_do_recebimento_a_queima() -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("caixa-ciclo", |pasta| {
            let mut ks = KeystoreDasGavetas::novo();
            b_com_cofre(pasta)?;
            let (a, mut sessao_a) = a_conversa_com_b()?;
            let texto = b"MENSAGEM-SECRETA-PARA-PROCURAR-NO-DISCO";
            let carta = carta_para_b(&sessao_a.cifrar(texto)?)?;

            let gaveta = chegar(&ks, &carta)?;
            assert_eq!(
                caixa()?,
                vec![ItemDaCaixa {
                    gaveta,
                    numero: 1,
                    lida: false
                }]
            );
            // selada: o texto não está no disco
            assert_eq!(achados_no_disco(pasta, texto)?, 0);
            // o relay entregou de novo: reconhecida, sem gastar gaveta
            assert_eq!(receber_carta(&carta)?, Chegada::JaGuardada);

            // ler: remetente e texto; marcada como lida no disco
            let t0 = Instant::now();
            let lida = ler_pelo_app(&ks, gaveta, t0)?;
            assert_eq!(lida.remetente, a.codigo_contato());
            assert_eq!(lida.texto.as_slice(), texto);
            assert!(caixa()?[0].lida);
            let antes = caixa::ler_gaveta(pasta, gaveta)?;

            // antes do minuto não queima; no minuto, queima
            assert!(queimar_vencidas_em(t0 + Duration::from_secs(59))?.is_empty());
            assert_eq!(queimar_vencidas_em(t0 + TEMPO_ATE_QUEIMAR)?, vec![gaveta]);
            assert!(caixa()?.is_empty());
            assert_ne!(
                caixa::ler_gaveta(pasta, gaveta)?,
                antes,
                "gaveta não foi sobrescrita"
            );
            assert_eq!(
                pedido_de_leitura(gaveta).err(),
                Some(ErroCaixa::Estado),
                "queimada ainda lê"
            );

            // o Kotlin troca a chave: mesmo com os bytes antigos de volta, não abre
            let pedido_velho = caixa::pedido(gaveta, &caixa::nonce_da_gaveta(&antes)?);
            ks.trocar(gaveta);
            chave_trocada(gaveta)?;
            assert_eq!(
                caixa::abrir_gaveta(gaveta, &antes, &ks.hmac(gaveta, &pedido_velho)).err(),
                Some(ErroCofre::GavetaNaoAbre)
            );
            // depois da queima, o texto não está em lugar nenhum da pasta
            assert_eq!(achados_no_disco(pasta, texto)?, 0);
            // calibração da busca: o mesmo texto gravado em claro é achado
            fs::write(pasta.join("vazamento.txt"), texto)?;
            assert!(
                achados_no_disco(pasta, texto)? >= 1,
                "a busca não acha nem em claro"
            );
            fs::remove_file(pasta.join("vazamento.txt"))?;

            // a gaveta volta a servir, com a chave nova
            let g2 = chegar(&ks, &carta_para_b(&sessao_a.cifrar(b"de novo")?)?)?;
            assert_eq!(g2, gaveta);
            assert_eq!(
                ler_pelo_app(&ks, g2, Instant::now())?.texto.as_slice(),
                b"de novo"
            );
            Ok(())
        })
    }

    #[test]
    fn caixa_mensagem_comum_e_conversa_encerrada() -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("caixa-comum", |pasta| {
            let ks = KeystoreDasGavetas::novo();
            b_com_cofre(pasta)?;
            let (a, mut sessao_a) = a_conversa_com_b()?;
            let g1 = chegar(&ks, &carta_para_b(&sessao_a.cifrar(b"oi")?)?)?;
            // B responde; A decifra; agora A manda mensagem comum
            let resposta = cifrar(a.codigo_contato(), b"oi, A")?;
            assert_eq!(sessao_a.decifrar(&resposta)?.as_slice(), b"oi, A");
            let comum = sessao_a.cifrar(b"comum")?;
            assert!(matches!(comum, OlmMessage::Normal(_)));
            let g2 = chegar(&ks, &carta_para_b(&comum)?)?;
            assert_eq!(
                ler_pelo_app(&ks, g2, Instant::now())?.texto.as_slice(),
                b"comum"
            );

            // o app fecha: a conversa morre, a caixa fica
            reabrir(pasta)?;
            assert_eq!(nao_abertas()?, 0);
            // a lida antes de fechar queima ao abrir; a não lida continua
            assert_eq!(queimar_lidas()?, vec![g2]);
            assert_eq!(
                caixa()?.iter().map(|i| i.gaveta).collect::<Vec<_>>(),
                vec![g1]
            );
            assert_eq!(
                ler_pelo_app(&ks, g1, Instant::now())?.texto.as_slice(),
                b"oi"
            );

            // mensagem comum de conversa encerrada: não abre, entra na conta
            let perdida = carta_para_b(&sessao_a.cifrar(b"perdida")?)?;
            assert_eq!(receber_carta(&perdida)?, Chegada::NaoAbre);
            assert_eq!(nao_abertas()?, 1);
            assert_eq!(caixa()?.len(), 1, "carta que não abriu ocupou gaveta");
            Ok(())
        })
    }

    #[test]
    fn caixa_recusas_antes_de_decifrar() -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("caixa-recusas", |pasta| {
            let ks = KeystoreDasGavetas::novo();
            // sem cofre aberto
            esquecer();
            assert_eq!(caixa().err(), Some(ErroCaixa::SemIdentidade));
            gerar_identidade();
            assert_eq!(caixa().err(), Some(ErroCaixa::CofreFechado));

            b_com_cofre(pasta)?;
            let (_a, mut sessao_a) = a_conversa_com_b()?;
            let m = sessao_a.cifrar(b"x")?;
            // para outra pessoa
            let mut outra = carta_para_b(&m)?;
            outra[1] ^= 1;
            assert_eq!(receber_carta(&outra)?, Chegada::NaoEParaMim);
            // estragada
            assert!(matches!(
                receber_carta(&[1, 2, 3]),
                Err(ErroCaixa::Carta(_))
            ));
            // remetente fora da agenda (primeira mensagem de quem não está nela)
            let (estranho, b_para_estranho) = a_que_leu_o_qr_de_b()?;
            let avulsa = nova_chave_avulsa()?;
            let de_estranho = Sessao::abrir(&estranho, &b_para_estranho, &avulsa)?.cifrar(b"?")?;
            assert_eq!(
                receber_carta(&carta_para_b(&de_estranho)?)?,
                Chegada::NaoAbre
            );
            // adulterada: último byte da mensagem
            let mut adulterada = carta_para_b(&m)?;
            let fim = adulterada.len() - 1;
            adulterada[fim] ^= 1;
            assert_eq!(receber_carta(&adulterada)?, Chegada::NaoAbre);
            assert_eq!(nao_abertas()?, 2);
            // a original continua abrindo: as recusas não andaram a conversa
            chegar(&ks, &carta_para_b(&m)?)?;
            // chegada pendente bloqueia a próxima
            let m2 = carta_para_b(&sessao_a.cifrar(b"y")?)?;
            assert!(matches!(receber_carta(&m2)?, Chegada::Selar { .. }));
            let m3 = carta_para_b(&sessao_a.cifrar(b"z")?)?;
            assert_eq!(receber_carta(&m3).err(), Some(ErroCaixa::ChegadaPendente));
            desistir_da_chegada()?;
            assert_eq!(selar_chegada(&[0u8; 32]).err(), Some(ErroCaixa::SemChegada));
            // a reservada que desistiu queima no queimar_lidas
            assert_eq!(queimar_lidas()?.len(), 1);
            assert_eq!(
                pedido_de_leitura(200).err(),
                Some(ErroCaixa::GavetaInvalida)
            );
            Ok(())
        })
    }

    #[test]
    fn caixa_cheia_nao_abre_a_carta_e_aceita_depois() -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("caixa-cheia", |pasta| {
            let mut ks = KeystoreDasGavetas::novo();
            b_com_cofre(pasta)?;
            let (_a, mut sessao_a) = a_conversa_com_b()?;
            for i in 0..caixa::CAPACIDADE {
                chegar(
                    &ks,
                    &carta_para_b(&sessao_a.cifrar(format!("m{i}").as_bytes())?)?,
                )?;
            }
            let carta = carta_para_b(&sessao_a.cifrar(b"a 101")?)?;
            assert_eq!(receber_carta(&carta)?, Chegada::CaixaCheia);
            assert_eq!(nao_abertas()?, 0);
            // libera uma gaveta: ler, queimar, trocar a chave
            let g = caixa()?[0].gaveta;
            ler_pelo_app(&ks, g, Instant::now())?;
            assert_eq!(queimar_lidas()?, vec![g]);
            ks.trocar(g);
            chave_trocada(g)?;
            // a mesma carta agora entra: a recusa por caixa cheia não a abriu
            let g2 = chegar(&ks, &carta)?;
            assert_eq!(
                ler_pelo_app(&ks, g2, Instant::now())?.texto.as_slice(),
                b"a 101"
            );
            assert_eq!(fs::read_dir(pasta)?.count(), 104);
            Ok(())
        })
    }

    #[test]
    fn caixa_falha_ao_selar_mantem_a_carta_esperando() -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("caixa-falha", |pasta| {
            let ks = KeystoreDasGavetas::novo();
            b_com_cofre(pasta)?;
            let (_a, mut sessao_a) = a_conversa_com_b()?;
            let Chegada::Selar { gaveta, pedido } =
                receber_carta(&carta_para_b(&sessao_a.cifrar(b"guardar")?)?)?
            else {
                return Err("esperava Selar".into());
            };
            let nome = pasta.join(caixa::nome_gaveta(gaveta));
            let vazia = fs::read(&nome)?;
            fs::remove_file(&nome)?;
            fs::create_dir(&nome)?;
            assert!(matches!(
                selar_chegada(&ks.hmac(gaveta, &pedido)),
                Err(ErroCaixa::Cofre(ErroCofre::Disco(_)))
            ));
            assert!(caixa()?.is_empty(), "entrou sem gravar");
            fs::remove_dir(&nome)?;
            fs::write(&nome, &vazia)?;
            assert_eq!(selar_chegada(&ks.hmac(gaveta, &pedido))?, gaveta);
            assert_eq!(
                ler_pelo_app(&ks, gaveta, Instant::now())?.texto.as_slice(),
                b"guardar"
            );
            Ok(())
        })
    }

    #[test]
    fn caixa_maior_texto_cabe_e_o_seguinte_e_recusado() -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("caixa-maior", |pasta| {
            let ks = KeystoreDasGavetas::novo();
            b_com_cofre(pasta)?;
            let (a, mut sessao_a) = a_conversa_com_b()?;
            let grande = vec![b'k'; caixa::MAXIMO_TEXTO];
            let carta = carta_para_b(&sessao_a.cifrar(&grande)?)?;
            let g = chegar(&ks, &carta)?;
            assert_eq!(
                ler_pelo_app(&ks, g, Instant::now())?.texto.as_slice(),
                grande.as_slice()
            );
            // do lado de quem manda, o núcleo recusa um byte a mais
            let resposta = cifrar(a.codigo_contato(), b"ok")?;
            sessao_a.decifrar(&resposta)?;
            assert_eq!(
                cifrar(a.codigo_contato(), &vec![0u8; caixa::MAXIMO_TEXTO + 1]).err(),
                Some(ErroConversa::TextoGrande)
            );
            Ok(())
        })
    }

    #[test]
    fn caixa_indice_adulterado_nao_abre_o_cofre() -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("caixa-adulterada", |pasta| {
            b_com_cofre(pasta)?;
            let mut indice = caixa::ler_indice(pasta)?.ok_or("sem índice")?;
            indice[100] ^= 1;
            fs::write(pasta.join(caixa::NOME_INDICE), &indice)?;
            esquecer();
            let e = preparar_abertura_com(pasta, PIN, |_| {})?;
            assert_eq!(
                abrir_cofre(pasta, &keystore_falso(e.as_ref())).err(),
                Some(ErroCofre::CaixaCorrompida)
            );
            assert_eq!(codigo_atual(), None);
            // cofre de antes da caixa (sem índice): abre com a caixa nova, vazia
            fs::remove_file(pasta.join(caixa::NOME_INDICE))?;
            reabrir(pasta)?;
            assert!(caixa()?.is_empty());
            assert!(caixa::ler_indice(pasta)?.is_some());
            Ok(())
        })
    }

    #[test]
    fn carta_cifrada_vai_para_o_contato_e_abre_do_outro_lado()
    -> Result<(), Box<dyn std::error::Error>> {
        com_pasta("carta-saida", |pasta| {
            let ks = KeystoreDasGavetas::novo();
            b_com_cofre(pasta)?;
            let (a, mut sessao_a) = a_conversa_com_b()?;
            chegar(&ks, &carta_para_b(&sessao_a.cifrar(b"oi")?)?)?;
            let bytes = cifrar_carta(a.codigo_contato(), b"resposta de B")?;
            let carta = Carta::ler(&bytes)?;
            assert_eq!(carta.destinatario, *a.chaves_publicas().ed25519.as_bytes());
            assert_eq!(carta.tipo, kryptos_protocolo::carta::Tipo::Normal);
            let m = crate::sessao::ler_mensagem(usize::from(carta.tipo.numero()), &carta.mensagem)?;
            assert_eq!(sessao_a.decifrar(&m)?.as_slice(), b"resposta de B");
            // contato sem conversa aberta e texto grande não viram carta
            let (outro, _) = a_que_leu_o_qr_de_b()?;
            assert_eq!(
                cifrar_carta(outro.codigo_contato(), b"x").err(),
                Some(ErroConversa::ContatoDesconhecido)
            );
            assert_eq!(
                cifrar_carta(a.codigo_contato(), &vec![0u8; caixa::MAXIMO_TEXTO + 1]).err(),
                Some(ErroConversa::TextoGrande)
            );
            Ok(())
        })
    }
}
