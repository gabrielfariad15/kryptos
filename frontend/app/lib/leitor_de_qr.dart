// Liga os quadros da câmera à agenda, um quadro por vez.
//
// A câmera manda quadros mais rápido do que o núcleo lê em alguns aparelhos. Enquanto
// um quadro está no núcleo, os que chegam são descartados, não enfileirados: fila
// cresce, gasta memória e atrasa a resposta para o quadro que já passou.
//
// Também mede, para a regra de 9 em 10 leituras em menos de 2 s
// (cartao-de-contato.md): tempo desde a câmera ligada, quadros lidos e descartados, e
// o tempo médio de um quadro no núcleo. Nada do QR vai para a medida.

import 'dart:async';

import 'package:flutter/foundation.dart';

import 'contatos.dart';

/// Números de uma leitura, para a tela e para o logcat.
class MedidaDaLeitura {
  const MedidaDaLeitura({
    required this.tempo,
    required this.lidos,
    required this.descartados,
    required this.noNucleo,
  });

  /// Desde a câmera ligada até o fim (achou, cancelou).
  final Duration tempo;

  /// Quadros que passaram pelo núcleo.
  final int lidos;

  /// Quadros que chegaram com o núcleo ocupado.
  final int descartados;

  /// Tempo médio de um quadro no núcleo (inclui a cópia pela ponte).
  final Duration noNucleo;

  /// Linha do logcat que o ferramentas/medir-leitura-qr.ps1 conta.
  String linha(String desfecho) =>
      'Kryptos: MEDIDA qr $desfecho em ${tempo.inMilliseconds} ms, '
      '$lidos quadros lidos, $descartados descartados, '
      'nucleo ${noNucleo.inMilliseconds} ms por quadro';
}

class LeitorDeQr {
  LeitorDeQr({
    required this.ler,
    required this.aoAchar,
    required this.aoRecusar,
    Stopwatch? relogio,
  }) : _relogio = relogio ?? Stopwatch() {
    _relogio.start();
  }

  final Future<ContatoLido?> Function(Quadro quadro) ler;

  /// Chamado uma vez, com o contato aceito. Depois disso os quadros são ignorados.
  final void Function(ContatoLido contato, MedidaDaLeitura medida) aoAchar;

  /// Chamado a cada quadro com QR recusado. A leitura continua.
  final void Function(String codigo) aoRecusar;

  final Stopwatch _relogio;
  bool _ocupado = false;
  bool _parado = false;
  int _lidos = 0;
  int _descartados = 0;
  Duration _noNucleo = Duration.zero;
  String? _ultimaRecusa;

  bool get parado => _parado;

  MedidaDaLeitura medida() => MedidaDaLeitura(
    tempo: _relogio.elapsed,
    lidos: _lidos,
    descartados: _descartados,
    noNucleo: _lidos == 0 ? Duration.zero : _noNucleo ~/ _lidos,
  );

  /// Entrega um quadro da câmera. Volta na hora: a leitura corre por fora.
  void receber(Quadro quadro) {
    if (_parado) return;
    if (_ocupado) {
      _descartados++;
      return;
    }
    _ocupado = true;
    unawaited(_ler(quadro));
  }

  /// Para de aceitar quadros e devolve a medida até aqui.
  MedidaDaLeitura parar() {
    _parado = true;
    _relogio.stop();
    return medida();
  }

  Future<void> _ler(Quadro quadro) async {
    final tempo = Stopwatch()..start();
    try {
      final contato = await ler(quadro);
      _contar(tempo);
      if (_parado || contato == null) return;
      aoAchar(contato, parar());
    } on QrRecusado catch (e) {
      _contar(tempo);
      if (_parado) return;
      if (e.codigo != _ultimaRecusa) {
        // Só o código fixo; repete-se a cada quadro, então vai ao log uma vez.
        debugPrint(
          'Kryptos: qr recusado (${e.codigo}) em ${_relogio.elapsedMilliseconds} ms',
        );
        _ultimaRecusa = e.codigo;
      }
      aoRecusar(e.codigo);
    } catch (e) {
      // Falha fora do previsto: só o tipo, nada do quadro. Vai para a tela também,
      // para não ficar uma câmera que nunca lê sem dizer por quê. A leitura segue.
      _contar(tempo);
      debugPrint('Kryptos: falha ao ler o quadro (${e.runtimeType})');
      if (!_parado) aoRecusar('FALHA_${e.runtimeType}');
    } finally {
      _ocupado = false;
    }
  }

  void _contar(Stopwatch tempo) {
    _lidos++;
    _noNucleo += tempo.elapsed;
  }
}
