// Testes do plantão (ciclo de vida, busca, queima, envio) no computador, com mensageiro
// e relógio falsos (apoio.dart). O núcleo, o Keystore e o relay de verdade ficam para
// os testes no aparelho e para o teste com os dois celulares.

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kryptos/plantao.dart';
import 'package:kryptos/relay.dart';

import 'apoio.dart';

const amigo = 'K7QX-M2ND';

/// Muda o ciclo de vida passando pelos estados do meio, como o motor do Flutter faz
/// no aparelho (resumed, inactive, hidden, paused).
Future<void> mudarCiclo(WidgetTester tester, AppLifecycleState estado) async {
  const ordem = [
    AppLifecycleState.resumed,
    AppLifecycleState.inactive,
    AppLifecycleState.hidden,
    AppLifecycleState.paused,
  ];
  var i = ordem.indexOf(
    tester.binding.lifecycleState ?? AppLifecycleState.resumed,
  );
  final alvo = ordem.indexOf(estado);
  while (i != alvo) {
    i += alvo > i ? 1 : -1;
    tester.binding.handleAppLifecycleStateChanged(ordem[i]);
    for (var k = 0; k < 5; k++) {
      await tester.pump();
    }
  }
}

void main() {
  testWidgets(
    'ao abrir: prepara a caixa, publica as chaves e busca, nessa ordem',
    (tester) async {
      final m = MensageiroFalso()..noRelay.add((amigo, 'oi'));
      final p = await abrirCentral(
        tester,
        mensageiro: m,
        relogio: RelogioFalso(),
      );
      expect(m.chamadas.take(3), ['preparar', 'publicar', 'buscar']);
      expect(p.seladas, hasLength(1));
      expect(p.novasDe(amigo), 1);
      expect(p.ligado, isTrue);
    },
  );

  testWidgets('busca de novo a cada 10 s com o app na frente', (tester) async {
    final relogio = RelogioFalso();
    final m = MensageiroFalso();
    final p = await abrirCentral(tester, mensageiro: m, relogio: relogio);
    final antes = m.chamadas.where((c) => c == 'buscar').length;
    m.noRelay.add((amigo, 'chegou depois'));
    await passar(tester, relogio, const Duration(seconds: 9));
    expect(m.chamadas.where((c) => c == 'buscar').length, antes);
    expect(p.seladas, isEmpty);
    await passar(tester, relogio, const Duration(seconds: 1));
    expect(m.chamadas.where((c) => c == 'buscar').length, antes + 1);
    expect(p.novasDe(amigo), 1);
  });

  testWidgets('sem relay: não publica, não busca e não manda', (tester) async {
    final relogio = RelogioFalso();
    final m = MensageiroFalso(temRelay: false)..guardar(amigo, 'antiga');
    final p = await abrirCentral(tester, mensageiro: m, relogio: relogio);
    await passar(tester, relogio, const Duration(seconds: 25));
    expect(m.chamadas, isNot(contains('publicar')));
    expect(m.chamadas, isNot(contains('buscar')));
    expect(p.seladas, hasLength(1), reason: 'a caixa continua legível');
    expect(await p.enviar(amigo, 'oi'), 'SEM_RELAY');
    expect(m.enviadas, isEmpty);
  });

  testWidgets(
    'app no fundo: para a busca, queima as lidas e apaga as enviadas zeradas',
    (tester) async {
      final relogio = RelogioFalso();
      final m = MensageiroFalso()..guardar(amigo, 'segredo');
      final p = await abrirCentral(tester, mensageiro: m, relogio: relogio);
      final aberta = await p.abrir(p.seladas.single.gaveta);
      expect(aberta, isNotNull);
      expect(await p.enviar(amigo, 'resposta'), isNull);
      final enviada = p.enviadasPara(amigo).single;

      await mudarCiclo(tester, AppLifecycleState.inactive);
      expect(p.naFrente, isFalse);
      expect(p.ligado, isTrue, reason: 'só sair da frente não desliga');
      await mudarCiclo(tester, AppLifecycleState.hidden);
      expect(p.ligado, isFalse);
      expect(m.chamadas.last, 'queimarLidas');
      expect(p.seladas, isEmpty, reason: 'a lida queimou ao sair');
      expect(p.enviadasPara(amigo), isEmpty);
      expect(enviada.texto.every((b) => b == 0), isTrue);

      final buscas = m.chamadas.where((c) => c == 'buscar').length;
      await mudarCiclo(tester, AppLifecycleState.paused);
      await passar(tester, relogio, const Duration(seconds: 30));
      expect(m.chamadas.where((c) => c == 'buscar').length, buscas);
    },
  );

  testWidgets('voltar para a frente liga de novo e busca na hora', (
    tester,
  ) async {
    final relogio = RelogioFalso();
    final m = MensageiroFalso();
    final p = await abrirCentral(tester, mensageiro: m, relogio: relogio);
    await mudarCiclo(tester, AppLifecycleState.paused);
    expect(p.ligado, isFalse);
    m.noRelay.add((amigo, 'enquanto fora'));
    m.chamadas.clear();
    await mudarCiclo(tester, AppLifecycleState.resumed);
    expect(p.ligado, isTrue);
    expect(p.naFrente, isTrue);
    expect(m.chamadas.take(3), ['preparar', 'publicar', 'buscar']);
    expect(p.novasDe(amigo), 1);
  });

  testWidgets('a enviada queima 60 s depois de enviada, com o texto zerado', (
    tester,
  ) async {
    final relogio = RelogioFalso();
    final m = MensageiroFalso();
    final p = await abrirCentral(tester, mensageiro: m, relogio: relogio);
    expect(await p.enviar(amigo, 'some em um minuto'), isNull);
    expect(m.enviadas.single.$1, amigo);
    final enviada = p.enviadasPara(amigo).single;
    expect(p.segundosAteQueimar(enviada.em), 60);
    await passar(tester, relogio, const Duration(seconds: 59));
    expect(p.enviadasPara(amigo), hasLength(1));
    expect(p.segundosAteQueimar(enviada.em), 1);
    await passar(tester, relogio, const Duration(seconds: 1));
    expect(p.enviadasPara(amigo), isEmpty);
    expect(enviada.texto.every((b) => b == 0), isTrue);
  });

  testWidgets('a lida só é queimada pelo núcleo depois de um minuto', (
    tester,
  ) async {
    final relogio = RelogioFalso();
    final m = MensageiroFalso()
      ..guardar(amigo, 'leia')
      ..vencidas = true;
    final p = await abrirCentral(tester, mensageiro: m, relogio: relogio);
    final gaveta = p.seladas.single.gaveta;
    await p.abrir(gaveta);
    expect(p.seladas.single.lida, isTrue);
    expect(p.novasDe(amigo), 0);
    expect(p.lidaEm(gaveta), relogio.agora);
    await passar(tester, relogio, const Duration(seconds: 59));
    expect(p.seladas, hasLength(1), reason: 'queimou antes do minuto');
    await passar(tester, relogio, const Duration(seconds: 1));
    expect(p.seladas, isEmpty);
    expect(p.lidaEm(gaveta), isNull);
  });

  testWidgets('falha do relay aparece com o código e some na busca seguinte', (
    tester,
  ) async {
    final relogio = RelogioFalso();
    final m = MensageiroFalso()
      ..falhaNaBusca = const FalhaDoRelay('REDE_SocketException');
    final p = await abrirCentral(tester, mensageiro: m, relogio: relogio);
    expect(p.falhaDoRelay, 'REDE_SocketException');
    m.falhaNaBusca = null;
    await passar(tester, relogio, const Duration(seconds: 10));
    expect(p.falhaDoRelay, isNull);
  });

  testWidgets('texto acima de 65.536 bytes é recusado sem chegar ao núcleo', (
    tester,
  ) async {
    final m = MensageiroFalso();
    final p = await abrirCentral(
      tester,
      mensageiro: m,
      relogio: RelogioFalso(),
    );
    expect(
      await p.enviar(amigo, 'a' * (bytesMaximoDoTexto + 1)),
      'TEXTO_GRANDE',
    );
    expect(await p.enviar(amigo, ''), 'TEXTO_VAZIO');
    expect(m.enviadas, isEmpty);
    // "ç" tem 2 bytes em UTF-8: o limite é em bytes, não em letras.
    expect(
      await p.enviar(amigo, 'ç' * (bytesMaximoDoTexto ~/ 2 + 1)),
      'TEXTO_GRANDE',
    );
    expect(await p.enviar(amigo, 'a' * bytesMaximoDoTexto), isNull);
    expect(m.enviadas.single.$2, hasLength(bytesMaximoDoTexto));
  });

  testWidgets('envio que falha devolve o código e não guarda a enviada', (
    tester,
  ) async {
    final m = MensageiroFalso()..falhaNoEnvio = const FalhaDoRelay('SEM_CHAVE');
    final p = await abrirCentral(
      tester,
      mensageiro: m,
      relogio: RelogioFalso(),
    );
    expect(await p.enviar(amigo, 'oi'), 'SEM_CHAVE');
    expect(p.enviadasPara(amigo), isEmpty);
  });
}
