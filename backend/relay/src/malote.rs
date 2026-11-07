//! Malote: as cartas esperando o destinatário (documentacao/arquitetura/malote.md).
//!
//! - Depositar: qualquer um deixa uma carta para um destinatário. O relay não sabe
//!   quem mandou: a carta não diz, e o relay não lê o IP.
//! - Buscar: só o destinatário, com a busca assinada (`kryptos_protocolo::malote`). A
//!   mesma busca confirma as cartas já guardadas no aparelho, que o relay apaga, e
//!   entrega as próximas.
//! - Carta não confirmada é entregue de novo na busca seguinte. Depois de
//!   `VALIDADE` sem confirmação, é apagada.
//!
//! Guardado no disco do relay (redb), para sobreviver a um reinício. No disco ficam a
//! carta cifrada, o destinatário, a hora do depósito e o número da última busca.
//!
//! Tabelas:
//!
//! ```text
//! CARTAS   destinatário (32) || sequência (8, BE)  →  id (16) || depósito (8, LE) || carta
//! DESTINOS destinatário (32)  →  último número (8) || cartas (4) || bytes (8) || mexido (8)
//! META     "sequencia", "bytes"  →  u64
//! ```
//!
//! A sequência é do relay inteiro e só sobe: entrega na ordem de chegada. O id é
//! sorteado, para quem busca não deduzir quantas cartas o relay recebeu.

use std::path::Path;
use std::time::Duration;

use kryptos_protocolo::BYTES_CHAVE;
use kryptos_protocolo::carta::Carta;
use kryptos_protocolo::malote::{BYTES_ID, BYTES_MAXIMO_ENTREGA, Busca, Entrega};
use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};

/// Uma carta não confirmada é apagada depois disto (decisão de 07/11/2026).
pub const VALIDADE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Cartas por destinatário. Estimo: 500 cobre uma semana de conversa de quem não abre
/// o app, e um remetente sozinho não enche o disco do relay.
pub const MAXIMO_CARTAS_POR_DESTINATARIO: u32 = 500;

/// Bytes por destinatário. Estimo: 8 MiB, cerca de 120 cartas do tamanho máximo.
pub const MAXIMO_BYTES_POR_DESTINATARIO: u64 = 8 << 20;

/// Destinatários com cartas no relay. Estimo, como no estoque.
pub const MAXIMO_DESTINATARIOS: u64 = 20_000;

/// Bytes de carta no relay inteiro. Estimo: 2 GiB.
pub const MAXIMO_BYTES_TOTAL: u64 = 2 << 30;

const CARTAS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("cartas");
const DESTINOS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("destinos");
const META: TableDefinition<&str, u64> = TableDefinition::new("meta");

const BYTES_CHAVE_CARTA: usize = BYTES_CHAVE + 8;
const BYTES_CABECALHO_VALOR: usize = BYTES_ID + 8;
const BYTES_DESTINO: usize = 8 + 4 + 8 + 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroMalote {
    /// O destinatário já tem cartas ou bytes demais esperando.
    MaloteCheio,
    /// O relay chegou ao limite de destinatários ou de bytes.
    RelayCheio,
    /// Número da busca igual ou menor que o último aceito: devolve o último.
    NumeroVelho { ultimo: u64 },
    /// Erro do banco no disco do relay (registrado no log, sem dados de ninguém).
    Disco,
    /// Sem sorteio de id.
    Aleatorio,
}

impl ErroMalote {
    pub fn codigo(&self) -> &'static str {
        match self {
            Self::MaloteCheio => "MALOTE_CHEIO",
            Self::RelayCheio => "RELAY_CHEIO",
            Self::NumeroVelho { .. } => "NUMERO_VELHO",
            Self::Disco => "DISCO",
            Self::Aleatorio => "SEM_ALEATORIO",
        }
    }
}

impl std::fmt::Display for ErroMalote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErroMalote {}

fn disco(e: impl Into<redb::Error>) -> ErroMalote {
    // O texto do erro do redb fala do banco, nunca do conteúdo.
    tracing::error!("malote: erro no banco: {}", e.into());
    ErroMalote::Disco
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Destino {
    ultimo_numero: u64,
    cartas: u32,
    bytes: u64,
    /// Última vez (segundos Unix) que algo mudou para este destinatário.
    mexido: u64,
}

impl Destino {
    fn novo(agora: u64) -> Self {
        Self {
            ultimo_numero: 0,
            cartas: 0,
            bytes: 0,
            mexido: agora,
        }
    }

    fn para_bytes(&self) -> [u8; BYTES_DESTINO] {
        let mut b = [0u8; BYTES_DESTINO];
        b[..8].copy_from_slice(&self.ultimo_numero.to_le_bytes());
        b[8..12].copy_from_slice(&self.cartas.to_le_bytes());
        b[12..20].copy_from_slice(&self.bytes.to_le_bytes());
        b[20..].copy_from_slice(&self.mexido.to_le_bytes());
        b
    }

    fn de_bytes(b: &[u8]) -> Result<Self, ErroMalote> {
        if b.len() != BYTES_DESTINO {
            tracing::error!("malote: registro de destino com tamanho errado");
            return Err(ErroMalote::Disco);
        }
        let u64_de = |p: &[u8]| {
            let mut q = [0u8; 8];
            q.copy_from_slice(p);
            u64::from_le_bytes(q)
        };
        let mut c = [0u8; 4];
        c.copy_from_slice(&b[8..12]);
        Ok(Self {
            ultimo_numero: u64_de(&b[..8]),
            cartas: u32::from_le_bytes(c),
            bytes: u64_de(&b[12..20]),
            mexido: u64_de(&b[20..]),
        })
    }
}

fn chave_da_carta(destinatario: &[u8; BYTES_CHAVE], sequencia: u64) -> [u8; BYTES_CHAVE_CARTA] {
    let mut k = [0u8; BYTES_CHAVE_CARTA];
    k[..BYTES_CHAVE].copy_from_slice(destinatario);
    k[BYTES_CHAVE..].copy_from_slice(&sequencia.to_be_bytes());
    k
}

/// Carta guardada, lida do banco.
struct Guardada {
    chave: Vec<u8>,
    id: [u8; BYTES_ID],
    deposito: u64,
    carta: Vec<u8>,
}

fn ler_valor(chave: &[u8], valor: &[u8]) -> Result<Guardada, ErroMalote> {
    if valor.len() <= BYTES_CABECALHO_VALOR {
        tracing::error!("malote: carta guardada com tamanho errado");
        return Err(ErroMalote::Disco);
    }
    let mut id = [0u8; BYTES_ID];
    id.copy_from_slice(&valor[..BYTES_ID]);
    let mut d = [0u8; 8];
    d.copy_from_slice(&valor[BYTES_ID..BYTES_CABECALHO_VALOR]);
    Ok(Guardada {
        chave: chave.to_vec(),
        id,
        deposito: u64::from_le_bytes(d),
        carta: valor[BYTES_CABECALHO_VALOR..].to_vec(),
    })
}

fn venceu(deposito: u64, agora: u64) -> bool {
    agora.saturating_sub(deposito) >= VALIDADE.as_secs()
}

/// Limites do malote. O relay usa `Limites::PADRAO`; os testes usam limites menores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limites {
    pub cartas_por_destinatario: u32,
    pub bytes_por_destinatario: u64,
    pub destinatarios: u64,
    pub bytes_total: u64,
}

impl Limites {
    pub const PADRAO: Self = Self {
        cartas_por_destinatario: MAXIMO_CARTAS_POR_DESTINATARIO,
        bytes_por_destinatario: MAXIMO_BYTES_POR_DESTINATARIO,
        destinatarios: MAXIMO_DESTINATARIOS,
        bytes_total: MAXIMO_BYTES_TOTAL,
    };
}

pub struct Malote {
    banco: Database,
    limites: Limites,
}

impl Malote {
    /// Malote num arquivo: as cartas sobrevivem a um reinício do relay.
    pub fn em_arquivo(caminho: &Path) -> Result<Self, ErroMalote> {
        Self::preparar(Database::create(caminho).map_err(disco)?, Limites::PADRAO)
    }

    /// Malote só na memória (testes, ou relay sem arquivo configurado).
    pub fn em_memoria() -> Result<Self, ErroMalote> {
        Self::em_memoria_com(Limites::PADRAO)
    }

    pub fn em_memoria_com(limites: Limites) -> Result<Self, ErroMalote> {
        let banco = redb::Builder::new()
            .create_with_backend(redb::backends::InMemoryBackend::new())
            .map_err(disco)?;
        Self::preparar(banco, limites)
    }

    fn preparar(banco: Database, limites: Limites) -> Result<Self, ErroMalote> {
        // Cria as tabelas, para a primeira leitura não falhar por tabela ausente.
        let t = banco.begin_write().map_err(disco)?;
        {
            t.open_table(CARTAS).map_err(disco)?;
            t.open_table(DESTINOS).map_err(disco)?;
            t.open_table(META).map_err(disco)?;
        }
        t.commit().map_err(disco)?;
        Ok(Self { banco, limites })
    }

    /// Guarda a carta para o destinatário dela. `agora` em segundos Unix.
    pub fn depositar(&self, carta: &Carta, bytes: &[u8], agora: u64) -> Result<(), ErroMalote> {
        let mut id = [0u8; BYTES_ID];
        getrandom::fill(&mut id).map_err(|_| ErroMalote::Aleatorio)?;
        let t = self.banco.begin_write().map_err(disco)?;
        {
            let mut cartas = t.open_table(CARTAS).map_err(disco)?;
            let mut destinos = t.open_table(DESTINOS).map_err(disco)?;
            let mut meta = t.open_table(META).map_err(disco)?;
            let dest = carta.destinatario;
            let existente = destinos
                .get(dest.as_slice())
                .map_err(disco)?
                .map(|v| Destino::de_bytes(v.value()))
                .transpose()?;
            let mut destino = match existente {
                Some(d) => d,
                None => {
                    if destinos.len().map_err(disco)? >= self.limites.destinatarios {
                        return Err(ErroMalote::RelayCheio);
                    }
                    Destino::novo(agora)
                }
            };
            let mut total = meta.get("bytes").map_err(disco)?.map_or(0, |v| v.value());
            let (removidas, bytes_removidos) = apagar_vencidas_de(&mut cartas, &dest, agora)?;
            destino.cartas = destino.cartas.saturating_sub(removidas);
            destino.bytes = destino.bytes.saturating_sub(bytes_removidos);
            total = total.saturating_sub(bytes_removidos);
            let tamanho = u64::try_from(bytes.len()).map_err(|_| ErroMalote::MaloteCheio)?;
            if destino.cartas >= self.limites.cartas_por_destinatario
                || destino.bytes + tamanho > self.limites.bytes_por_destinatario
            {
                return Err(ErroMalote::MaloteCheio);
            }
            if total + tamanho > self.limites.bytes_total {
                return Err(ErroMalote::RelayCheio);
            }
            let sequencia = meta
                .get("sequencia")
                .map_err(disco)?
                .map_or(0, |v| v.value())
                + 1;
            let mut valor = Vec::with_capacity(BYTES_CABECALHO_VALOR + bytes.len());
            valor.extend_from_slice(&id);
            valor.extend_from_slice(&agora.to_le_bytes());
            valor.extend_from_slice(bytes);
            cartas
                .insert(
                    chave_da_carta(&dest, sequencia).as_slice(),
                    valor.as_slice(),
                )
                .map_err(disco)?;
            destino.cartas += 1;
            destino.bytes += tamanho;
            destino.mexido = agora;
            destinos
                .insert(dest.as_slice(), destino.para_bytes().as_slice())
                .map_err(disco)?;
            meta.insert("sequencia", sequencia).map_err(disco)?;
            meta.insert("bytes", total + tamanho).map_err(disco)?;
        }
        t.commit().map_err(disco)
    }

    /// Atende uma busca já conferida: aceita o número, apaga as confirmadas e as
    /// vencidas, e entrega as próximas na ordem de chegada, até `BYTES_MAXIMO_ENTREGA`.
    /// Destinatário sem registro: entrega vazia e nada é gravado.
    pub fn buscar(&self, busca: &Busca, agora: u64) -> Result<Entrega, ErroMalote> {
        let t = self.banco.begin_write().map_err(disco)?;
        let entrega;
        {
            let mut cartas = t.open_table(CARTAS).map_err(disco)?;
            let mut destinos = t.open_table(DESTINOS).map_err(disco)?;
            let mut meta = t.open_table(META).map_err(disco)?;
            let dest = busca.destinatario;
            let Some(mut destino) = destinos
                .get(dest.as_slice())
                .map_err(disco)?
                .map(|v| Destino::de_bytes(v.value()))
                .transpose()?
            else {
                return Ok(Entrega::default());
            };
            if busca.numero <= destino.ultimo_numero {
                return Err(ErroMalote::NumeroVelho {
                    ultimo: destino.ultimo_numero,
                });
            }
            destino.ultimo_numero = busca.numero;
            destino.mexido = agora;

            let todas = guardadas_de(&cartas, &dest)?;
            let mut removidas = 0u32;
            let mut bytes_removidos = 0u64;
            let mut restam = Vec::new();
            for g in todas {
                if busca.confirmadas.contains(&g.id) || venceu(g.deposito, agora) {
                    cartas.remove(g.chave.as_slice()).map_err(disco)?;
                    removidas += 1;
                    bytes_removidos += u64::try_from(g.carta.len()).unwrap_or(u64::MAX);
                } else {
                    restam.push(g);
                }
            }
            destino.cartas = destino.cartas.saturating_sub(removidas);
            destino.bytes = destino.bytes.saturating_sub(bytes_removidos);
            let total = meta.get("bytes").map_err(disco)?.map_or(0, |v| v.value());
            meta.insert("bytes", total.saturating_sub(bytes_removidos))
                .map_err(disco)?;
            destinos
                .insert(dest.as_slice(), destino.para_bytes().as_slice())
                .map_err(disco)?;

            let mut e = Entrega::default();
            let mut usados = 0usize;
            for g in restam {
                let item = Entrega::bytes_do_item(&g.carta);
                if usados + item > BYTES_MAXIMO_ENTREGA {
                    break;
                }
                usados += item;
                e.cartas.push((g.id, g.carta));
            }
            entrega = e;
        }
        t.commit().map_err(disco)?;
        Ok(entrega)
    }

    /// Varredura do relay inteiro: apaga as cartas vencidas e os destinatários sem
    /// carta parados há mais de `VALIDADE`. Devolve quantas cartas saíram.
    pub fn limpar(&self, agora: u64) -> Result<u64, ErroMalote> {
        let t = self.banco.begin_write().map_err(disco)?;
        let mut total_removidas = 0u64;
        {
            let mut cartas = t.open_table(CARTAS).map_err(disco)?;
            let mut destinos = t.open_table(DESTINOS).map_err(disco)?;
            let mut meta = t.open_table(META).map_err(disco)?;
            let lista: Vec<(Vec<u8>, Destino)> = destinos
                .iter()
                .map_err(disco)?
                .map(|r| {
                    let (k, v) = r.map_err(disco)?;
                    Ok((k.value().to_vec(), Destino::de_bytes(v.value())?))
                })
                .collect::<Result<_, ErroMalote>>()?;
            let mut total = meta.get("bytes").map_err(disco)?.map_or(0, |v| v.value());
            for (chave, mut destino) in lista {
                let dest: [u8; BYTES_CHAVE] = chave.as_slice().try_into().map_err(|_| {
                    tracing::error!("malote: destinatário com tamanho errado");
                    ErroMalote::Disco
                })?;
                let (removidas, bytes) = apagar_vencidas_de(&mut cartas, &dest, agora)?;
                total_removidas += u64::from(removidas);
                total = total.saturating_sub(bytes);
                destino.cartas = destino.cartas.saturating_sub(removidas);
                destino.bytes = destino.bytes.saturating_sub(bytes);
                if destino.cartas == 0 && venceu(destino.mexido, agora) {
                    destinos.remove(chave.as_slice()).map_err(disco)?;
                } else if removidas > 0 {
                    destinos
                        .insert(chave.as_slice(), destino.para_bytes().as_slice())
                        .map_err(disco)?;
                }
            }
            meta.insert("bytes", total).map_err(disco)?;
        }
        t.commit().map_err(disco)?;
        Ok(total_removidas)
    }

    /// Quantas cartas e quantos bytes o relay guarda (para os testes e o log de saúde).
    pub fn tamanho(&self) -> Result<(u64, u64), ErroMalote> {
        let t = self.banco.begin_read().map_err(disco)?;
        let cartas = t.open_table(CARTAS).map_err(disco)?;
        let meta = t.open_table(META).map_err(disco)?;
        Ok((
            cartas.len().map_err(disco)?,
            meta.get("bytes").map_err(disco)?.map_or(0, |v| v.value()),
        ))
    }
}

fn faixa(dest: &[u8; BYTES_CHAVE]) -> ([u8; BYTES_CHAVE_CARTA], [u8; BYTES_CHAVE_CARTA]) {
    (chave_da_carta(dest, 0), chave_da_carta(dest, u64::MAX))
}

fn guardadas_de(
    cartas: &impl ReadableTable<&'static [u8], &'static [u8]>,
    dest: &[u8; BYTES_CHAVE],
) -> Result<Vec<Guardada>, ErroMalote> {
    let (inicio, fim) = faixa(dest);
    cartas
        .range::<&[u8]>(inicio.as_slice()..=fim.as_slice())
        .map_err(disco)?
        .map(|r| {
            let (k, v) = r.map_err(disco)?;
            ler_valor(k.value(), v.value())
        })
        .collect()
}

fn apagar_vencidas_de(
    cartas: &mut redb::Table<&'static [u8], &'static [u8]>,
    dest: &[u8; BYTES_CHAVE],
    agora: u64,
) -> Result<(u32, u64), ErroMalote> {
    let mut removidas = 0u32;
    let mut bytes = 0u64;
    for g in guardadas_de(cartas, dest)? {
        if venceu(g.deposito, agora) {
            cartas.remove(g.chave.as_slice()).map_err(disco)?;
            removidas += 1;
            bytes += u64::try_from(g.carta.len()).unwrap_or(u64::MAX);
        }
    }
    Ok((removidas, bytes))
}

#[cfg(test)]
mod testes {
    use kryptos_protocolo::carta::Tipo;

    use super::*;

    type R = Result<(), Box<dyn std::error::Error>>;

    const T0: u64 = 1_800_000_000;
    const DIA: u64 = 24 * 60 * 60;

    fn carta(dest: u8, conteudo: &[u8]) -> Result<(Carta, Vec<u8>), Box<dyn std::error::Error>> {
        let c = Carta {
            destinatario: [dest; 32],
            tipo: Tipo::Normal,
            mensagem: conteudo.to_vec(),
        };
        let b = c.codificar()?;
        Ok((Carta::ler(&b)?, b))
    }

    fn deposita(
        m: &Malote,
        dest: u8,
        conteudo: &[u8],
        agora: u64,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let (c, b) = carta(dest, conteudo)?;
        m.depositar(&c, &b, agora)?;
        Ok(b)
    }

    fn busca(dest: u8, numero: u64, confirmadas: Vec<[u8; BYTES_ID]>) -> Busca {
        Busca {
            destinatario: [dest; 32],
            numero,
            confirmadas,
        }
    }

    #[test]
    fn entrega_na_ordem_ate_confirmar() -> R {
        let m = Malote::em_memoria()?;
        let c1 = deposita(&m, 1, b"um", T0)?;
        let c2 = deposita(&m, 1, b"dois", T0 + 1)?;
        let outra = deposita(&m, 2, b"de outro", T0)?;
        assert_eq!(m.tamanho()?.0, 3);

        let e = m.buscar(&busca(1, 1, vec![]), T0 + 2)?;
        assert_eq!(
            e.cartas.iter().map(|(_, c)| c.clone()).collect::<Vec<_>>(),
            vec![c1.clone(), c2.clone()]
        );
        assert_ne!(e.cartas[0].0, e.cartas[1].0, "ids iguais");
        // sem confirmar, volta igual
        let de_novo = m.buscar(&busca(1, 2, vec![]), T0 + 3)?;
        assert_eq!(de_novo, e);
        // confirma a primeira: só a segunda volta
        let e3 = m.buscar(&busca(1, 3, vec![e.cartas[0].0]), T0 + 4)?;
        assert_eq!(e3.cartas, vec![e.cartas[1].clone()]);
        // confirma a segunda (e um id que não existe): vazio
        let e4 = m.buscar(&busca(1, 4, vec![e.cartas[1].0, [0xEE; BYTES_ID]]), T0 + 5)?;
        assert!(e4.cartas.is_empty());
        // a do outro destinatário não foi tocada
        let e5 = m.buscar(&busca(2, 1, vec![]), T0 + 6)?;
        assert_eq!(e5.cartas.len(), 1);
        assert_eq!(e5.cartas[0].1, outra);
        let bytes_restantes = u64::try_from(outra.len())?;
        assert_eq!(m.tamanho()?, (1, bytes_restantes));
        Ok(())
    }

    #[test]
    fn numero_so_sobe_e_destinatario_sem_registro_nao_grava() -> R {
        // Um destinatário só cabe: se a busca de quem não tem carta gravasse um
        // registro, o depósito seguinte daria RELAY_CHEIO.
        let m = Malote::em_memoria_com(Limites {
            destinatarios: 1,
            ..Limites::PADRAO
        })?;
        assert!(m.buscar(&busca(8, 5, vec![]), T0)?.cartas.is_empty());
        // nada gravado: cabe o destinatário 9, e para o 8 o número 1 ainda valeria
        deposita(&m, 9, b"x", T0)?;
        m.buscar(&busca(9, 1, vec![]), T0)?;
        assert_eq!(
            m.buscar(&busca(9, 1, vec![]), T0).err(),
            Some(ErroMalote::NumeroVelho { ultimo: 1 })
        );
        assert_eq!(
            m.buscar(&busca(9, 0, vec![]), T0).err(),
            Some(ErroMalote::NumeroVelho { ultimo: 1 })
        );
        // a busca recusada não apagou nada
        assert_eq!(m.buscar(&busca(9, 7, vec![]), T0)?.cartas.len(), 1);
        Ok(())
    }

    #[test]
    fn carta_vence_em_sete_dias() -> R {
        let m = Malote::em_memoria()?;
        deposita(&m, 3, b"velha", T0)?;
        assert_eq!(
            m.buscar(&busca(3, 1, vec![]), T0 + 7 * DIA - 1)?
                .cartas
                .len(),
            1
        );
        assert!(
            m.buscar(&busca(3, 2, vec![]), T0 + 7 * DIA)?
                .cartas
                .is_empty()
        );
        assert_eq!(m.tamanho()?, (0, 0));
        Ok(())
    }

    #[test]
    fn limpar_tira_vencidas_e_destinatarios_parados() -> R {
        let m = Malote::em_memoria_com(Limites {
            destinatarios: 2,
            ..Limites::PADRAO
        })?;
        deposita(&m, 1, b"a", T0)?;
        deposita(&m, 2, b"b", T0 + 3 * DIA)?;
        // relay cheio de destinatários
        assert_eq!(
            deposita(&m, 3, b"c", T0 + 3 * DIA)
                .err()
                .and_then(|e| e.downcast_ref::<ErroMalote>().copied()),
            Some(ErroMalote::RelayCheio)
        );
        assert_eq!(m.limpar(T0 + 7 * DIA)?, 1);
        assert_eq!(m.tamanho()?.0, 1);
        // o destinatário 1 ficou sem carta, mas mexido há 7 dias: sai, e abre vaga
        deposita(&m, 3, b"c", T0 + 7 * DIA)?;
        assert_eq!(m.limpar(T0 + 10 * DIA)?, 1);
        assert_eq!(m.tamanho()?.0, 1);
        Ok(())
    }

    #[test]
    fn limites_por_destinatario_e_do_relay() -> R {
        let m = Malote::em_memoria_com(Limites {
            cartas_por_destinatario: 3,
            bytes_por_destinatario: 1_000,
            destinatarios: 100,
            bytes_total: 2_000,
        })?;
        let cheio = |r: Result<Vec<u8>, Box<dyn std::error::Error>>| {
            r.err()
                .and_then(|e| e.downcast_ref::<ErroMalote>().copied())
        };
        for _ in 0..3 {
            deposita(&m, 1, b"x", T0)?;
        }
        assert_eq!(
            cheio(deposita(&m, 1, b"x", T0)),
            Some(ErroMalote::MaloteCheio)
        );
        // bytes do destinatário
        deposita(&m, 2, &[0u8; 600], T0)?;
        assert_eq!(
            cheio(deposita(&m, 2, &[0u8; 600], T0)),
            Some(ErroMalote::MaloteCheio)
        );
        // bytes do relay inteiro
        deposita(&m, 3, &[0u8; 900], T0)?;
        assert_eq!(
            cheio(deposita(&m, 4, &[0u8; 400], T0)),
            Some(ErroMalote::RelayCheio)
        );
        // confirmar libera lugar
        let e = m.buscar(&busca(1, 1, vec![]), T0)?;
        m.buscar(
            &busca(1, 2, e.cartas.iter().map(|(id, _)| *id).collect()),
            T0,
        )?;
        deposita(&m, 1, b"x", T0)?;
        Ok(())
    }

    #[test]
    fn entrega_respeita_o_tamanho_maximo() -> R {
        let m = Malote::em_memoria()?;
        let grande = vec![7u8; kryptos_protocolo::carta::MAXIMO_MENSAGEM];
        for _ in 0..20 {
            deposita(&m, 5, &grande, T0)?;
        }
        let e = m.buscar(&busca(5, 1, vec![]), T0)?;
        let bytes = e.codificar()?.len();
        assert!(bytes <= 2 + BYTES_MAXIMO_ENTREGA, "{bytes}");
        assert!(e.cartas.len() < 20 && !e.cartas.is_empty());
        let ids: Vec<_> = e.cartas.iter().map(|(id, _)| *id).collect();
        let resto = m.buscar(&busca(5, 2, ids), T0)?;
        assert_eq!(e.cartas.len() + resto.cartas.len(), 20);
        Ok(())
    }

    #[test]
    fn arquivo_sobrevive_ao_reinicio_com_o_numero() -> R {
        let pasta = std::env::temp_dir().join(format!("kryptos-malote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta)?;
        let caminho = pasta.join("malote.redb");
        let r = (|| -> R {
            {
                let m = Malote::em_arquivo(&caminho)?;
                deposita(&m, 4, b"guardada", T0)?;
                m.buscar(&busca(4, 10, vec![]), T0)?;
            }
            let m = Malote::em_arquivo(&caminho)?;
            assert_eq!(
                m.buscar(&busca(4, 10, vec![]), T0).err(),
                Some(ErroMalote::NumeroVelho { ultimo: 10 })
            );
            let e = m.buscar(&busca(4, 11, vec![]), T0)?;
            assert_eq!(e.cartas.len(), 1);
            Ok(())
        })();
        std::fs::remove_dir_all(&pasta)?;
        r
    }
}
