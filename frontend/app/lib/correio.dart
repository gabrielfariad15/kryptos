// Correio: o núcleo, a caixa e o relay juntos. Publica as chaves avulsas, abre conversa
// com a chave avulsa do contato, manda cartas e busca as que chegaram.
// Decisões: documentacao/arquitetura/malote.md.
//
// Números da publicação e da busca: o relay só aceita número maior que o último. O app
// não guarda número nenhum: usa os milissegundos do relógio e, se o relay responder
// NUMERO_VELHO <último>, tenta uma vez com o seguinte.
//
// Nada de texto, carta, dono ou código de contato em log (regra 8.2).

import 'package:flutter/foundation.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';

import 'caixa_do_aparelho.dart';
import 'cofre.dart';
import 'mensageiro.dart';
import 'relay.dart';
import 'src/rust/api/nucleo.dart' as nucleo;

export 'mensageiro.dart' show Recebidas;

/// Quantas chaves avulsas manter no relay. Estimo: 50 conversas novas antes de o app
/// abrir de novo; abaixo de 20, repõe.
const chavesNoRelay = 50;
const minimoDeChavesNoRelay = 20;

class Correio {
  Correio({required this.relay, required this.caixa});

  final ClienteDoRelay relay;
  final CaixaDoAparelho caixa;

  static int _numero() => DateTime.now().millisecondsSinceEpoch;

  /// Chama [pedido] com um número novo; com NUMERO_VELHO, tenta uma vez com o seguinte.
  static Future<T> _comNumero<T>(Future<T> Function(int numero) pedido) async {
    try {
      return await pedido(_numero());
    } on FalhaDoRelay catch (e) {
      final ultimo = e.numero;
      if (e.codigo != 'NUMERO_VELHO' || ultimo == null) rethrow;
      return pedido(ultimo + 1);
    }
  }

  /// Consulta o estoque e, abaixo de [minimo], publica até chegar a [manter].
  /// Devolve quantas chaves ficaram no relay.
  ///
  /// A consulta (sem chave) acerta o número antes: assim a publicação com chaves sai
  /// com um número já aceito mais um e não gera chave avulsa que nunca seria publicada.
  Future<int> publicarChaves({
    int manter = chavesNoRelay,
    int minimo = minimoDeChavesNoRelay,
  }) async {
    var numero = _numero();
    Future<int> consultar() async => relay.publicar(
      await _nucleo(() => nucleo.relayPublicacao(quantas: 0, numero: numero)),
    );
    int ha;
    try {
      ha = await consultar();
    } on FalhaDoRelay catch (e) {
      final ultimo = e.numero;
      if (e.codigo != 'NUMERO_VELHO' || ultimo == null) rethrow;
      numero = ultimo + 1;
      ha = await consultar();
    }
    if (ha >= minimo) return ha;
    final faltam = manter - ha;
    final publicacao = await _nucleo(
      () => nucleo.relayPublicacao(quantas: faltam, numero: numero + 1),
    );
    return relay.publicar(publicacao);
  }

  /// Abre a conversa com o contato usando uma chave avulsa dele no relay.
  Future<void> abrirConversa(String codigo) async {
    final dono = _nucleoSync(() => nucleo.donoDoContato(codigo: codigo));
    final avulsa = await relay.retirar(dono);
    await _nucleo(() => nucleo.abrirConversa(codigo: codigo, avulsa: avulsa));
  }

  /// Cifra o texto para o contato e deixa a carta no relay. Sem conversa aberta nesta
  /// execução, abre uma antes.
  Future<void> enviar(String codigo, List<int> texto) async {
    final nova = !_nucleoSync(() => nucleo.temConversa(codigo: codigo));
    if (nova) {
      await abrirConversa(codigo);
    }
    final carta = await _nucleo(
      () => nucleo.cifrarCarta(codigo: codigo, texto: texto),
    );
    await relay.depositar(carta);
    // Só o caminho, para o teste nos celulares (regra 8.2: nada de texto ou código).
    debugPrint(
      'Kryptos: carta enviada (${nova ? 'conversa nova' : 'conversa aberta'})',
    );
  }

  /// Busca as cartas no relay, sela cada uma na caixa e confirma as que não precisam
  /// voltar. Repete até o relay entregar vazio ou a caixa encher.
  Future<Recebidas> buscar() async {
    var seladas = 0;
    var naoAbriram = 0;
    var confirmar = <Uint8List>[];
    while (true) {
      final ids = confirmar;
      final entrega = await _comNumero(
        (n) async => relay.buscar(
          await _nucleo(() => nucleo.relayBusca(numero: n, confirmadas: ids)),
        ),
      );
      final cartas = await _nucleo(
        () => nucleo.relayLerEntrega(entrega: entrega),
      );
      confirmar = <Uint8List>[];
      if (cartas.isEmpty) {
        return Recebidas(
          seladas: seladas,
          naoAbriram: naoAbriram,
          caixaCheia: false,
        );
      }
      for (final c in cartas) {
        switch (await caixa.receber(c.carta)) {
          case Chegada.selada:
            seladas++;
            confirmar.add(c.id);
          case Chegada.jaGuardada:
          case Chegada.naoEParaMim:
            confirmar.add(c.id);
          case Chegada.naoAbre:
            naoAbriram++;
            confirmar.add(c.id);
          case Chegada.caixaCheia:
            // Confirma as que já entraram e para: o resto fica no relay.
            await _confirmarSo(confirmar);
            return Recebidas(
              seladas: seladas,
              naoAbriram: naoAbriram,
              caixaCheia: true,
            );
        }
      }
    }
  }

  Future<void> _confirmarSo(List<Uint8List> ids) async {
    if (ids.isEmpty) return;
    await _comNumero(
      (n) async => relay.buscar(
        await _nucleo(() => nucleo.relayBusca(numero: n, confirmadas: ids)),
      ),
    );
  }

  static T _nucleoSync<T>(T Function() f) {
    try {
      return f();
    } on AnyhowException catch (e) {
      throw FalhaDoCofre(e.message.trim().split(RegExp(r'\s')).first);
    }
  }

  static Future<T> _nucleo<T>(Future<T> Function() f) async {
    try {
      return await f();
    } on AnyhowException catch (e) {
      throw FalhaDoCofre(e.message.trim().split(RegExp(r'\s')).first);
    }
  }
}
