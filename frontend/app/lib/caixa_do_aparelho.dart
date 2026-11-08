// Caixa selada de verdade: núcleo em Rust (selagem, índice, queima) + Keystore do
// Android (uma chave HMAC por gaveta, trocada a cada queima).
// Formato e etapas: documentacao/arquitetura/caixa-selada.md.
//
// Nada de texto, chave ou código de contato em log (regra 8.2).

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';

import 'cofre.dart';
import 'cofre_do_aparelho.dart';
import 'src/rust/api/nucleo.dart' as nucleo;

/// Gavetas da caixa: o mesmo número do núcleo (`caixa::CAPACIDADE`).
const capacidadeDaCaixa = 100;

/// Começo do alias das chaves das gavetas no Keystore.
const prefixoDasGavetas = 'kryptos-gaveta-';

/// O que aconteceu com uma carta que chegou.
enum Chegada {
  /// Selada numa gaveta: pode confirmar ao relay.
  selada,

  /// Já estava numa gaveta (o relay entregou de novo): pode confirmar.
  jaGuardada,

  /// Não abre (conversa encerrada, remetente fora da agenda, adulterada): confirmar.
  naoAbre,

  /// Para outra pessoa: ignorar.
  naoEParaMim,

  /// Sem gaveta livre: não confirmar; a carta fica no relay.
  caixaCheia,
}

class CaixaDoAparelho {
  CaixaDoAparelho({required this.chave, this.prefixo = prefixoDasGavetas});

  final ChaveDoAparelho chave;
  final String prefixo;

  String alias(int gaveta) => '$prefixo${gaveta.toString().padLeft(2, '0')}';

  /// Cria as chaves de gaveta que faltam no Keystore. Depois da primeira vez, as 100
  /// existem sempre (a queima troca, não tira). Devolve quantas foram criadas agora.
  Future<int> prepararChaves() async {
    var criadas = 0;
    for (var g = 0; g < capacidadeDaCaixa; g++) {
      if (!await _traduzir(() => chave.existe(alias(g)))) {
        await _traduzir(() => chave.criar(alias(g)));
        criadas++;
      }
    }
    return criadas;
  }

  /// Porta única das cartas. Em [Chegada.selada], a carta já está gravada e conferida.
  /// Cada chegada deixa uma linha no logcat com o desfecho (sem texto nem código), para
  /// o teste nos celulares dizer o caminho de cada carta.
  /// Se o Keystore ou o disco falharem no meio, a carta é abandonada (o texto sai da
  /// memória) e a falha sobe: a gaveta reservada queima no próximo [queimarLidas].
  Future<Chegada> receber(List<int> carta) async {
    final c = await _traduzir(() => nucleo.caixaReceberCarta(carta: carta));
    switch (c.tipo) {
      case 'SELAR':
        Uint8List? hmac;
        try {
          final h = await _traduzir(() => chave.hmac(alias(c.gaveta), c.pedido));
          hmac = h;
          await _traduzir(() => nucleo.caixaSelarChegada(hmac: h));
          debugPrint(
            'Kryptos: carta selada '
            '(${c.conversaNova ? 'conversa nova' : 'conversa aberta'})',
          );
          return Chegada.selada;
        } catch (_) {
          try {
            await _traduzir(nucleo.caixaDesistirDaChegada);
          } on FalhaDoCofre catch (e) {
            // A falha que importa é a de cima, que sobe; esta só fica registrada.
            debugPrint('Kryptos: desistir da chegada falhou (${e.codigo})');
          }
          rethrow;
        } finally {
          _zerar(hmac);
        }
      case 'JA_GUARDADA':
        debugPrint('Kryptos: carta ja guardada');
        return Chegada.jaGuardada;
      case 'NAO_ABRE':
        debugPrint('Kryptos: carta nao abriu');
        return Chegada.naoAbre;
      case 'NAO_E_PARA_MIM':
        debugPrint('Kryptos: carta para outra pessoa');
        return Chegada.naoEParaMim;
      case 'CAIXA_CHEIA':
        debugPrint('Kryptos: caixa cheia, carta ficou no relay');
        return Chegada.caixaCheia;
      default:
        throw FalhaDoCofre('CHEGADA_${c.tipo}');
    }
  }

  /// Mensagens não queimadas, na ordem de chegada. Sem conteúdo.
  Future<List<nucleo.MensagemNaCaixa>> listar() =>
      _traduzir(nucleo.caixaListar);

  /// Cartas que não abriram nesta execução do app.
  Future<int> naoAbertas() => _traduzir(nucleo.caixaNaoAbertas);

  /// Abre a mensagem para a tela. O núcleo marca como lida no disco antes de entregar,
  /// e o minuto até a queima começa a contar.
  Future<nucleo.MensagemLida> ler(int gaveta) async {
    final pedido = await _traduzir(
      () => nucleo.caixaPedidoDeLeitura(gaveta: gaveta),
    );
    Uint8List? hmac;
    try {
      final h = await _traduzir(() => chave.hmac(alias(gaveta), pedido));
      hmac = h;
      return await _traduzir(() => nucleo.caixaLer(gaveta: gaveta, hmac: h));
    } finally {
      _zerar(hmac);
    }
  }

  /// Queima na hora as lidas e termina as queimas interrompidas. Chamar ao abrir, ao
  /// sair do app e ao apagar a tela. Devolve quantas gavetas tiveram a chave trocada.
  Future<int> queimarLidas() async =>
      _trocarChaves(await _traduzir(nucleo.caixaQueimarLidas));

  /// Queima as lidas há um minuto ou mais. Chamar a cada segundo com o app aberto.
  Future<int> queimarVencidas() async =>
      _trocarChaves(await _traduzir(nucleo.caixaQueimarVencidas));

  /// A gaveta já foi sobrescrita pelo núcleo. Trocar a chave no hardware é o que torna
  /// inúteis as cópias antigas da gaveta; só depois ela volta a ficar livre.
  Future<int> _trocarChaves(Uint8List gavetas) async {
    for (final g in gavetas) {
      await _traduzir(() => chave.criar(alias(g)));
      await _traduzir(() => nucleo.caixaChaveTrocada(gaveta: g));
    }
    return gavetas.length;
  }

  static void _zerar(Uint8List? b) => b?.fillRange(0, b.length, 0);

  static Future<T> _traduzir<T>(Future<T> Function() f) async {
    try {
      return await f();
    } on AnyhowException catch (e) {
      throw FalhaDoCofre(e.message.trim().split(RegExp(r'\s')).first);
    } on PlatformException catch (e) {
      throw FalhaDoCofre(e.code);
    }
  }
}
