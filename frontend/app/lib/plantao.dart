// Plantão: o estado das conversas enquanto o cofre está aberto, e o ciclo de vida do
// app. Sem dependência da ponte: os testes usam um mensageiro falso.
//
// Regras (aprovadas em 07/11/2026):
//   - Ao abrir e ao voltar para a frente: termina as queimas interrompidas, prepara as
//     chaves das gavetas, repõe as chaves avulsas no relay e busca as cartas.
//   - Com o app na frente: busca no relay a cada 10 s; confere a queima a cada 1 s.
//   - Saindo da frente (outra tela por cima, aviso do sistema): esconde o texto aberto.
//   - App no fundo ou tela apagada: para a busca, queima na hora as lidas e apaga da
//     memória as mensagens enviadas.
//   - Enviadas: só na memória, queimam 60 s depois de enviadas.
//
// Uma operação de cada vez no núcleo (fila): a chegada de uma carta tem duas etapas
// (reservar a gaveta, selar com o Keystore), e uma leitura no meio delas seria recusada.
//
// Nada de texto, chave ou código de contato em log (regra 8.2).

import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

import 'cofre.dart';
import 'mensageiro.dart';
import 'relay.dart';

/// O mesmo minuto do núcleo (`aparelho::TEMPO_ATE_QUEIMAR`).
const tempoAteQueimar = Duration(seconds: 60);

/// Intervalo da busca no relay com o app na frente (aprovado em 07/11/2026).
const intervaloDaBusca = Duration(seconds: 10);

/// O mesmo limite do núcleo (`caixa::MAXIMO_TEXTO`).
const bytesMaximoDoTexto = 65536;

/// Mensagem que o app mandou nesta execução. Fica só aqui, na memória.
class Enviada {
  Enviada._(this.numero, this.para, this.texto, this.em);

  final int numero;
  final String para;

  /// Zerado na queima.
  final Uint8List texto;
  final DateTime em;
}

class Plantao extends ChangeNotifier {
  Plantao({required this.mensageiro, DateTime Function()? relogio})
    : _relogio = relogio ?? DateTime.now;

  final Mensageiro mensageiro;
  final DateTime Function() _relogio;

  AppLifecycleListener? _ciclo;
  Timer? _busca;
  Timer? _queima;
  Future<void> _fila = Future<void>.value();
  bool _buscando = false;
  bool _queimando = false;
  bool _descartado = false;

  bool _ligado = false;
  bool _naFrente = true;
  List<Selada> _seladas = const [];
  int _naoAbertas = 0;
  bool _caixaCheia = false;
  String? _falhaDoRelay;
  String? _falhaDaCaixa;
  final _chegouEm = <int, DateTime>{};
  final _lidaEm = <int, DateTime>{};
  final _enviadas = <Enviada>[];
  int _proximaEnviada = 1;

  /// Busca e queima ligadas (app na frente, cofre aberto).
  bool get ligado => _ligado;

  /// O app está na frente: texto aberto pode aparecer.
  bool get naFrente => _naFrente;
  bool get temRelay => mensageiro.temRelay;
  List<Selada> get seladas => List.unmodifiable(_seladas);
  int get naoAbertas => _naoAbertas;
  bool get caixaCheia => _caixaCheia;

  /// Último código de falha do relay ou da rede; some na próxima busca que der certo.
  String? get falhaDoRelay => _falhaDoRelay;

  /// Último código de falha da caixa (núcleo, Keystore, disco).
  String? get falhaDaCaixa => _falhaDaCaixa;

  DateTime agora() => _relogio();

  /// Quando a mensagem apareceu neste aparelho (nesta execução).
  DateTime? chegouEm(int gaveta) => _chegouEm[gaveta];

  /// Quando a mensagem foi aberta pela primeira vez nesta execução.
  DateTime? lidaEm(int gaveta) => _lidaEm[gaveta];

  int novasDe(String remetente) =>
      _seladas.where((s) => s.remetente == remetente && !s.lida).length;

  List<Selada> seladasDe(String remetente) =>
      _seladas.where((s) => s.remetente == remetente).toList();

  List<Enviada> enviadasPara(String codigo) =>
      _enviadas.where((e) => e.para == codigo).toList();

  /// Segundos até a queima, contados de [desde]; 0 quando já venceu.
  int segundosAteQueimar(DateTime desde) {
    final falta = tempoAteQueimar - agora().difference(desde);
    if (falta.isNegative) return 0;
    return (falta.inMilliseconds / 1000).ceil();
  }

  /// Chamar uma vez, logo depois de o cofre abrir.
  Future<void> iniciar() {
    _ciclo ??= AppLifecycleListener(
      onInactive: _saiuDaFrente,
      onResume: _voltouParaFrente,
      onHide: desligar,
    );
    return ligar();
  }

  /// Liga a busca e a queima e faz a rodada de abertura.
  Future<void> ligar() async {
    if (_ligado || _descartado) return;
    _ligado = true;
    _naFrente = true;
    _queima = Timer.periodic(const Duration(seconds: 1), (_) => _conferir());
    if (temRelay) {
      _busca = Timer.periodic(intervaloDaBusca, (_) => _buscarAgora());
    }
    notifyListeners();
    await _naFila(() async {
      await _tentar('preparar', mensageiro.preparar);
      if (temRelay) {
        await _tentar('publicar', mensageiro.publicarChaves);
      }
    });
    await _buscarAgora();
  }

  /// App no fundo ou tela apagada: para tudo, queima as lidas e esquece as enviadas.
  Future<void> desligar() async {
    _naFrente = false;
    if (!_ligado) return;
    _ligado = false;
    _busca?.cancel();
    _busca = null;
    _queima?.cancel();
    _queima = null;
    _esquecerEnviadas();
    _lidaEm.clear();
    notifyListeners();
    await _naFila(() async {
      await _tentar('queimar', mensageiro.queimarLidas);
      await _atualizar();
    });
  }

  void _saiuDaFrente() {
    if (!_naFrente) return;
    _naFrente = false;
    notifyListeners();
  }

  void _voltouParaFrente() {
    _naFrente = true;
    notifyListeners();
    ligar();
  }

  /// Abre a mensagem para a tela (segurar para ler). Nada se falhar ou se a gaveta já
  /// não estiver na caixa. Quem recebe zera o texto depois de usar.
  Future<Aberta?> abrir(int gaveta) => _naFila(() async {
    final i = _seladas.indexWhere((s) => s.gaveta == gaveta);
    if (i < 0) return null;
    Aberta? aberta;
    await _tentar('ler', () async {
      aberta = await mensageiro.ler(gaveta);
    });
    if (aberta == null) return null;
    final s = _seladas[i];
    if (!s.lida) {
      _lidaEm[gaveta] = agora();
      _seladas = [
        for (final x in _seladas)
          x.gaveta == gaveta
              ? Selada(gaveta: x.gaveta, lida: true, remetente: x.remetente)
              : x,
      ];
      notifyListeners();
    } else {
      // Lida numa execução anterior que não queimou: conta daqui.
      _lidaEm.putIfAbsent(gaveta, agora);
    }
    return aberta;
  });

  /// Cifra e manda. Devolve nada quando deu certo, ou o código da falha.
  Future<String?> enviar(String codigo, String texto) async {
    if (!temRelay) return 'SEM_RELAY';
    final bytes = Uint8List.fromList(utf8.encode(texto));
    if (bytes.isEmpty) return 'TEXTO_VAZIO';
    if (bytes.length > bytesMaximoDoTexto) {
      bytes.fillRange(0, bytes.length, 0);
      return 'TEXTO_GRANDE';
    }
    final falha = await _naFila(() async {
      try {
        await mensageiro.enviar(codigo, bytes);
        return null;
      } on FalhaDoRelay catch (e) {
        _registrar('enviar', e.codigo);
        return e.codigo;
      } on FalhaDoCofre catch (e) {
        _registrar('enviar', e.codigo);
        return e.codigo;
      }
    });
    if (falha != null) {
      bytes.fillRange(0, bytes.length, 0);
      return falha;
    }
    if (!_ligado) {
      // Saiu da frente enquanto mandava: a enviada não fica na memória.
      bytes.fillRange(0, bytes.length, 0);
      return null;
    }
    _enviadas.add(Enviada._(_proximaEnviada++, codigo, bytes, agora()));
    notifyListeners();
    return null;
  }

  Future<void> _buscarAgora() async {
    if (!_ligado || _buscando) return;
    _buscando = true;
    try {
      await _naFila(() async {
        if (!_ligado) return;
        var certo = true;
        if (temRelay) {
          certo = await _tentar('buscar', () async {
            final r = await mensageiro.buscar();
            if (_caixaCheia != r.caixaCheia) {
              _caixaCheia = r.caixaCheia;
              notifyListeners();
            }
          });
        }
        certo = await _atualizar() && certo;
        // Rodada inteira certa: as falhas antigas deixam de valer.
        if (certo && (_falhaDoRelay != null || _falhaDaCaixa != null)) {
          _falhaDoRelay = null;
          _falhaDaCaixa = null;
          notifyListeners();
        }
      });
    } finally {
      _buscando = false;
    }
  }

  /// A cada segundo: queima as enviadas vencidas, pede ao núcleo a queima das lidas
  /// vencidas e redesenha a contagem (só quando há o que contar).
  void _conferir() {
    if (!_ligado) return;
    final agora = this.agora();
    var mudou = false;
    _enviadas.removeWhere((e) {
      if (agora.difference(e.em) < tempoAteQueimar) return false;
      e.texto.fillRange(0, e.texto.length, 0);
      mudou = true;
      return true;
    });
    if (_lidaEm.isNotEmpty || _enviadas.isNotEmpty || mudou) notifyListeners();
    final vencida = _lidaEm.values.any(
      (t) => agora.difference(t) >= tempoAteQueimar,
    );
    if (vencida && !_queimando) {
      _queimando = true;
      _naFila(() async {
        var queimadas = 0;
        await _tentar('queimar', () async {
          queimadas = await mensageiro.queimarVencidas();
        });
        if (queimadas > 0) await _atualizar();
      }).whenComplete(() => _queimando = false);
    }
  }

  Future<bool> _atualizar() {
    return _tentar('listar', () async {
      final lista = await mensageiro.listar();
      final naoAbertas = await mensageiro.naoAbertas();
      final agora = this.agora();
      final gavetas = {for (final s in lista) s.gaveta};
      for (final g in gavetas) {
        _chegouEm.putIfAbsent(g, () => agora);
      }
      _chegouEm.removeWhere((g, _) => !gavetas.contains(g));
      _lidaEm.removeWhere((g, _) => !gavetas.contains(g));
      final mudou = naoAbertas != _naoAbertas || !_mesmaLista(lista, _seladas);
      _seladas = lista;
      _naoAbertas = naoAbertas;
      if (mudou) notifyListeners();
    });
  }

  static bool _mesmaLista(List<Selada> a, List<Selada> b) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i].gaveta != b[i].gaveta ||
          a[i].lida != b[i].lida ||
          a[i].remetente != b[i].remetente) {
        return false;
      }
    }
    return true;
  }

  /// Roda [f] e diz se deu certo. Falha conhecida (relay ou caixa) fica registrada
  /// com o código e aparece na Central até uma rodada de busca dar certo. Qualquer
  /// outra sobe.
  Future<bool> _tentar(String onde, Future<void> Function() f) async {
    try {
      await f();
      return true;
    } on FalhaDoRelay catch (e) {
      _falhaDoRelay = e.codigo;
      _registrar(onde, e.codigo);
    } on FalhaDoCofre catch (e) {
      _falhaDaCaixa = e.codigo;
      _registrar(onde, e.codigo);
    }
    return false;
  }

  void _registrar(String onde, String codigo) {
    debugPrint('Kryptos: $onde falhou ($codigo)');
    if (!_descartado) notifyListeners();
  }

  Future<T> _naFila<T>(Future<T> Function() f) {
    final r = _fila.then((_) => f());
    _fila = r.then<void>((_) {}, onError: (Object _) {});
    return r;
  }

  void _esquecerEnviadas() {
    for (final e in _enviadas) {
      e.texto.fillRange(0, e.texto.length, 0);
    }
    _enviadas.clear();
  }

  @override
  void notifyListeners() {
    if (!_descartado) super.notifyListeners();
  }

  @override
  void dispose() {
    if (_descartado) return;
    _descartado = true;
    _ligado = false;
    _busca?.cancel();
    _queima?.cancel();
    _ciclo?.dispose();
    _esquecerEnviadas();
    super.dispose();
  }
}
