//! Estoque de chaves avulsas, só em memória (documentacao/arquitetura/estoque-de-chaves.md).
//!
//! - Publicar: o dono soma chaves ao estoque dele. Nunca substitui: uma chave já
//!   retirada não pode voltar e ser entregue a outra pessoa.
//! - Retirar: entrega a chave mais antiga do dono uma vez só e a apaga, com limite por
//!   destinatário (balde de fichas pelo relógio monotônico).
//! - Dono desconhecido e estoque vazio dão a mesma resposta: o relay não conta a quem
//!   pergunta se alguém publicou.
//!
//! Não há chave reserva: uma mensagem feita com ela poderia ser repetida e aparecer
//! de novo depois de queimada (decisão de 07/11/2026).
//!
//! Nada aqui sabe o IP de ninguém. O limite por endereço de rede fica no proxy da
//! implantação.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use kryptos_protocolo::{BYTES_CHAVE, ChaveAssinada, Publicacao};

/// Chaves guardadas por dono. Estimo: 100 cobre o estoque que o app vai manter.
pub const MAXIMO_ESTOQUE: usize = 100;

/// Donos com estoque. Estimo: 20.000 × 100 chaves × 96 bytes dá cerca de 192 MB no
/// pior caso, sem contar a estrutura.
pub const MAXIMO_DONOS: usize = 20_000;

/// Retiradas seguidas por destinatário antes de o limite começar. Estimo.
pub const RETIRADAS_SEGUIDAS: u32 = 10;

/// Uma ficha nova a cada 6 minutos: 10 conversas novas por hora com a mesma pessoa,
/// depois das 10 seguidas. Estimo.
pub const INTERVALO_DA_FICHA: Duration = Duration::from_secs(360);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroEstoque {
    /// Número igual ou menor que o último aceito deste dono: devolve o último, para o
    /// dono continuar dali sem precisar guardar nada.
    NumeroVelho { ultimo: u64 },
    /// A publicação passaria do máximo: devolve quanto há, para o dono ajustar.
    EstoqueCheio { estoque: usize },
    /// A mesma chave duas vezes, na publicação ou já no estoque.
    ChaveRepetida,
    /// Donos demais no relay.
    RelayCheio,
    /// Nada para entregar (dono desconhecido ou estoque vazio: mesma resposta).
    SemChave,
    /// Retiradas demais para este destinatário; dá para tentar depois de `espera`.
    Limite { espera: Duration },
}

impl ErroEstoque {
    pub fn codigo(&self) -> &'static str {
        match self {
            Self::NumeroVelho { .. } => "NUMERO_VELHO",
            Self::EstoqueCheio { .. } => "ESTOQUE_CHEIO",
            Self::ChaveRepetida => "CHAVE_REPETIDA",
            Self::RelayCheio => "RELAY_CHEIO",
            Self::SemChave => "SEM_CHAVE",
            Self::Limite { .. } => "LIMITE",
        }
    }
}

struct Caixa {
    chaves: VecDeque<ChaveAssinada>,
    ultimo_numero: u64,
    fichas: u32,
    /// Quando a última ficha foi contada. Fichas novas saem do tempo desde então.
    contada_em: Instant,
}

impl Caixa {
    fn nova(agora: Instant) -> Self {
        Self {
            chaves: VecDeque::new(),
            ultimo_numero: 0,
            fichas: RETIRADAS_SEGUIDAS,
            contada_em: agora,
        }
    }

    fn repor_fichas(&mut self, agora: Instant) {
        let passou = agora.saturating_duration_since(self.contada_em);
        let novas = passou.as_secs() / INTERVALO_DA_FICHA.as_secs();
        if novas == 0 {
            return;
        }
        let novas = u32::try_from(novas).unwrap_or(u32::MAX);
        self.fichas = self.fichas.saturating_add(novas).min(RETIRADAS_SEGUIDAS);
        if self.fichas == RETIRADAS_SEGUIDAS {
            self.contada_em = agora;
        } else {
            self.contada_em += INTERVALO_DA_FICHA * novas;
        }
    }
}

#[derive(Default)]
pub struct Estoque {
    caixas: HashMap<[u8; BYTES_CHAVE], Caixa>,
}

impl Estoque {
    pub fn novo() -> Self {
        Self::default()
    }

    /// Soma as chaves de uma publicação já conferida. Devolve quantas o dono tem agora.
    /// Recusada, não muda nada (nem o último número).
    pub fn publicar(&mut self, p: Publicacao, agora: Instant) -> Result<usize, ErroEstoque> {
        if !self.caixas.contains_key(&p.dono) && self.caixas.len() >= MAXIMO_DONOS {
            return Err(ErroEstoque::RelayCheio);
        }
        let caixa = self
            .caixas
            .entry(p.dono)
            .or_insert_with(|| Caixa::nova(agora));
        let resultado = (|| {
            if p.numero <= caixa.ultimo_numero {
                return Err(ErroEstoque::NumeroVelho {
                    ultimo: caixa.ultimo_numero,
                });
            }
            if caixa.chaves.len() + p.chaves.len() > MAXIMO_ESTOQUE {
                return Err(ErroEstoque::EstoqueCheio {
                    estoque: caixa.chaves.len(),
                });
            }
            for (i, k) in p.chaves.iter().enumerate() {
                let repetida = p.chaves[..i].iter().any(|o| o.chave == k.chave)
                    || caixa.chaves.iter().any(|o| o.chave == k.chave);
                if repetida {
                    return Err(ErroEstoque::ChaveRepetida);
                }
            }
            caixa.chaves.extend(p.chaves.iter().copied());
            caixa.ultimo_numero = p.numero;
            Ok(caixa.chaves.len())
        })();
        // Caixa criada só para esta publicação recusada não fica ocupando lugar.
        let sobrou_vazia =
            resultado.is_err() && caixa.ultimo_numero == 0 && caixa.chaves.is_empty();
        if sobrou_vazia {
            self.caixas.remove(&p.dono);
        }
        resultado
    }

    /// Entrega a chave mais antiga do dono e a apaga.
    pub fn retirar(
        &mut self,
        dono: &[u8; BYTES_CHAVE],
        agora: Instant,
    ) -> Result<ChaveAssinada, ErroEstoque> {
        let caixa = self.caixas.get_mut(dono).ok_or(ErroEstoque::SemChave)?;
        if caixa.chaves.is_empty() {
            return Err(ErroEstoque::SemChave);
        }
        caixa.repor_fichas(agora);
        if caixa.fichas == 0 {
            let passou = agora.saturating_duration_since(caixa.contada_em);
            return Err(ErroEstoque::Limite {
                espera: INTERVALO_DA_FICHA.saturating_sub(passou),
            });
        }
        let chave = caixa.chaves.pop_front().ok_or(ErroEstoque::SemChave)?;
        caixa.fichas -= 1;
        Ok(chave)
    }

    /// Quantas chaves o dono tem no estoque.
    pub fn contagem(&self, dono: &[u8; BYTES_CHAVE]) -> usize {
        self.caixas.get(dono).map_or(0, |c| c.chaves.len())
    }

    pub fn donos(&self) -> usize {
        self.caixas.len()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn chave(n: u8) -> ChaveAssinada {
        ChaveAssinada {
            chave: [n; 32],
            assinatura: [n; 64],
        }
    }

    fn publicacao(dono: u8, numero: u64, chaves: &[u8]) -> Publicacao {
        Publicacao {
            dono: [dono; 32],
            numero,
            chaves: chaves.iter().map(|&n| chave(n)).collect(),
        }
    }

    #[test]
    fn publica_soma_e_retira_em_ordem_uma_vez_so() -> Result<(), ErroEstoque> {
        let t = Instant::now();
        let mut e = Estoque::novo();
        assert_eq!(e.publicar(publicacao(1, 1, &[10, 11]), t)?, 2);
        assert_eq!(e.publicar(publicacao(1, 2, &[12]), t)?, 3);
        assert_eq!(e.publicar(publicacao(1, 3, &[]), t)?, 3, "consulta");
        for esperada in [10, 11, 12] {
            assert_eq!(e.retirar(&[1; 32], t)?, chave(esperada));
        }
        assert_eq!(e.retirar(&[1; 32], t).err(), Some(ErroEstoque::SemChave));
        Ok(())
    }

    #[test]
    fn numero_velho_e_recusado_e_devolve_o_ultimo() -> Result<(), ErroEstoque> {
        let t = Instant::now();
        let mut e = Estoque::novo();
        assert_eq!(
            e.publicar(publicacao(1, 0, &[10]), t).err(),
            Some(ErroEstoque::NumeroVelho { ultimo: 0 }),
            "começa em 1"
        );
        assert_eq!(e.donos(), 0, "recusa não deixa caixa");
        e.publicar(publicacao(1, 5, &[10]), t)?;
        for repetido in [5, 4, 1] {
            assert_eq!(
                e.publicar(publicacao(1, repetido, &[11]), t).err(),
                Some(ErroEstoque::NumeroVelho { ultimo: 5 })
            );
        }
        assert_eq!(e.contagem(&[1; 32]), 1, "recusa não somou");
        e.publicar(publicacao(1, 6, &[11]), t)?;
        e.publicar(publicacao(1, u64::MAX, &[12]), t)?;
        assert_eq!(
            e.publicar(publicacao(1, u64::MAX, &[13]), t).err(),
            Some(ErroEstoque::NumeroVelho { ultimo: u64::MAX })
        );
        Ok(())
    }

    #[test]
    fn estoque_cheio_recusa_inteira_e_diz_quanto_tem() -> Result<(), ErroEstoque> {
        let t = Instant::now();
        let mut e = Estoque::novo();
        let cheio: Vec<u8> = (0..u8::try_from(MAXIMO_ESTOQUE).unwrap_or(100) - 1).collect();
        e.publicar(publicacao(1, 1, &cheio), t)?;
        assert_eq!(
            e.publicar(publicacao(1, 2, &[200, 201]), t).err(),
            Some(ErroEstoque::EstoqueCheio {
                estoque: MAXIMO_ESTOQUE - 1
            })
        );
        // O número recusado não foi gasto: a mesma publicação com uma chave entra.
        assert_eq!(e.publicar(publicacao(1, 2, &[200]), t)?, MAXIMO_ESTOQUE);
        Ok(())
    }

    #[test]
    fn chave_repetida_e_recusada() -> Result<(), ErroEstoque> {
        let t = Instant::now();
        let mut e = Estoque::novo();
        assert_eq!(
            e.publicar(publicacao(1, 1, &[10, 10]), t).err(),
            Some(ErroEstoque::ChaveRepetida)
        );
        e.publicar(publicacao(1, 1, &[10]), t)?;
        assert_eq!(
            e.publicar(publicacao(1, 2, &[11, 10]), t).err(),
            Some(ErroEstoque::ChaveRepetida)
        );
        assert_eq!(e.contagem(&[1; 32]), 1);
        Ok(())
    }

    #[test]
    fn dono_desconhecido_e_estoque_vazio_respondem_igual() -> Result<(), ErroEstoque> {
        let t = Instant::now();
        let mut e = Estoque::novo();
        e.publicar(publicacao(1, 1, &[]), t)?;
        assert_eq!(e.retirar(&[1; 32], t).err(), Some(ErroEstoque::SemChave));
        assert_eq!(e.retirar(&[2; 32], t).err(), Some(ErroEstoque::SemChave));
        Ok(())
    }

    #[test]
    fn donos_de_mais_e_recusado_sem_tirar_ninguem() -> Result<(), ErroEstoque> {
        let t = Instant::now();
        let mut e = Estoque::novo();
        for i in 0..MAXIMO_DONOS {
            let mut dono = [0u8; 32];
            dono[..8].copy_from_slice(&(i as u64).to_le_bytes());
            e.publicar(
                Publicacao {
                    dono,
                    numero: 1,
                    chaves: Vec::new(),
                },
                t,
            )?;
        }
        assert_eq!(
            e.publicar(publicacao(0xFF, 1, &[1]), t).err(),
            Some(ErroEstoque::RelayCheio)
        );
        assert_eq!(e.donos(), MAXIMO_DONOS);
        // Quem já está continua publicando.
        let mut primeiro = [0u8; 32];
        primeiro[..8].copy_from_slice(&0u64.to_le_bytes());
        e.publicar(
            Publicacao {
                dono: primeiro,
                numero: 2,
                chaves: vec![chave(1)],
            },
            t,
        )?;
        Ok(())
    }

    /// Limite por destinatário: 10 seguidas, depois 1 a cada 6 minutos, sem acumular
    /// mais que 10. O relógio é passado pelo teste.
    #[test]
    fn limite_por_destinatario_segue_o_relogio() -> Result<(), ErroEstoque> {
        let t0 = Instant::now();
        let mut e = Estoque::novo();
        let muitas: Vec<u8> = (0..60).collect();
        e.publicar(publicacao(1, 1, &muitas), t0)?;
        for _ in 0..RETIRADAS_SEGUIDAS {
            e.retirar(&[1; 32], t0)?;
        }
        assert_eq!(
            e.retirar(&[1; 32], t0).err(),
            Some(ErroEstoque::Limite {
                espera: INTERVALO_DA_FICHA
            })
        );
        let quase = t0 + INTERVALO_DA_FICHA - Duration::from_secs(1);
        assert_eq!(
            e.retirar(&[1; 32], quase).err(),
            Some(ErroEstoque::Limite {
                espera: Duration::from_secs(1)
            })
        );
        let t1 = t0 + INTERVALO_DA_FICHA;
        e.retirar(&[1; 32], t1)?;
        assert!(matches!(
            e.retirar(&[1; 32], t1),
            Err(ErroEstoque::Limite { .. })
        ));
        // Um dia parado não junta mais que 10.
        let t2 = t1 + Duration::from_secs(86_400);
        for _ in 0..RETIRADAS_SEGUIDAS {
            e.retirar(&[1; 32], t2)?;
        }
        assert!(matches!(
            e.retirar(&[1; 32], t2),
            Err(ErroEstoque::Limite { .. })
        ));
        // O limite de um dono não atinge outro.
        e.publicar(publicacao(2, 1, &[1]), t2)?;
        e.retirar(&[2; 32], t2)?;
        // Retirada recusada não gastou chave: 60 - 10 - 1 - 10 = 39.
        assert_eq!(e.contagem(&[1; 32]), 39);
        Ok(())
    }

    #[test]
    fn codigos_sao_distintos() {
        let todos = [
            ErroEstoque::NumeroVelho { ultimo: 0 },
            ErroEstoque::EstoqueCheio { estoque: 0 },
            ErroEstoque::ChaveRepetida,
            ErroEstoque::RelayCheio,
            ErroEstoque::SemChave,
            ErroEstoque::Limite {
                espera: Duration::ZERO,
            },
        ]
        .map(|e| e.codigo());
        let mut u = todos.to_vec();
        u.sort_unstable();
        u.dedup();
        assert_eq!(u.len(), todos.len());
    }
}
