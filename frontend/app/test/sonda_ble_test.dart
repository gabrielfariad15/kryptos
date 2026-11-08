// Tela da sonda BLE (lib/sonda_ble.dart), com o lado Android trocado por um canal falso:
// o que a tela pede ao Android, o que ela mostra do que o Android manda, e os casos de
// erro (sem permissão, Android recusando).

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kryptos/sonda_ble.dart';

import 'apoio.dart';

const canal = MethodChannel('kryptos/sonda-ble-teste');

class AndroidFalso {
  AndroidFalso({this.permite = true, this.recusarProcura = false});

  final bool permite;
  final bool recusarProcura;
  final chamadas = <MethodCall>[];

  Future<Object?> responder(MethodCall c) async {
    chamadas.add(c);
    switch (c.method) {
      case 'permissoes':
        return permite;
      case 'inventario':
        return 'modelo=moto_g15 api=35 coded=sim';
      case 'procurar':
        if (recusarProcura) throw PlatformException(code: 'ARGUMENTO');
        return null;
      default:
        return null;
    }
  }

  List<String> get metodos => [for (final c in chamadas) c.method];
}

Future<AndroidFalso> montar(WidgetTester tester, AndroidFalso android) async {
  tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
    canal,
    android.responder,
  );
  addTearDown(
    () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      canal,
      null,
    ),
  );
  await tester.binding.setSurfaceSize(const Size(432, 960));
  addTearDown(() => tester.binding.setSurfaceSize(null));
  await tester.pumpWidget(const SondaBleApp(canal: canal));
  await tester.pumpAndSettle();
  return android;
}

/// O Android mandando uma chamada para a tela.
Future<void> doAndroid(WidgetTester tester, String metodo, Object? args) async {
  await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
    canal.name,
    const StandardMethodCodec().encodeMethodCall(MethodCall(metodo, args)),
    (_) {},
  );
  await tester.pump();
}

void main() {
  setUpAll(carregarFontes);

  testWidgets('com permissão: pede o inventário e mostra', (tester) async {
    final a = await montar(tester, AndroidFalso());
    expect(a.metodos, ['permissoes', 'inventario']);
    expect(find.text('modelo=moto_g15 api=35 coded=sim'), findsOneWidget);
    expect(find.byKey(const Key('sonda-sem-permissao')), findsNothing);
  });

  testWidgets('procurar leva as variáveis escolhidas', (tester) async {
    final a = await montar(tester, AndroidFalso());
    await tester.tap(find.byKey(const Key('sonda-phy-2M')));
    await tester.tap(find.byKey(const Key('sonda-prioridade-ALTA')));
    await tester.tap(find.byKey(const Key('sonda-distancia-10')));
    await tester.pump();
    await tester.tap(find.byKey(const Key('sonda-procurar')));
    await tester.pumpAndSettle();
    final c = a.chamadas.last;
    expect(c.method, 'procurar');
    expect(c.arguments, {
      'phy': '2M',
      'prioridade': 'ALTA',
      'distancia': 10,
      'repeticoes': 5,
      'calibrar': false,
    });
  });

  testWidgets('calibrar: 3 repetições com sabotagem; ambos e anunciar', (
    tester,
  ) async {
    final a = await montar(tester, AndroidFalso());
    await tester.tap(find.byKey(const Key('sonda-phy-CODED')));
    await tester.pump();
    await tester.tap(find.byKey(const Key('sonda-calibrar')));
    await tester.pumpAndSettle();
    expect(a.chamadas.last.method, 'procurar');
    expect(a.chamadas.last.arguments, {
      'phy': 'CODED',
      'prioridade': 'PADRAO',
      'distancia': 1,
      'repeticoes': 3,
      'calibrar': true,
    });
    await tester.tap(find.byKey(const Key('sonda-ambos')));
    await tester.pumpAndSettle();
    expect(a.chamadas.last.method, 'ambos');
    await tester.tap(find.byKey(const Key('sonda-anunciar')));
    await tester.pumpAndSettle();
    expect(a.chamadas.last.method, 'anunciar');
  });

  testWidgets(
    'linhas do Android aparecem, a mais nova em cima; FALHA em laranja',
    (tester) async {
      await montar(tester, AndroidFalso());
      await doAndroid(
        tester,
        'linha',
        '10:00:00.000 MEDIDA sonda-ble achou ms=380',
      );
      await doAndroid(
        tester,
        'linha',
        '10:00:01.000 FALHA sonda-ble NAO_ACHOU_30S',
      );
      final textos = tester
          .widgetList<Text>(
            find.descendant(
              of: find.byKey(const Key('sonda-linhas')),
              matching: find.byType(Text),
            ),
          )
          .toList();
      expect(textos.map((t) => t.data), [
        '10:00:01.000 FALHA sonda-ble NAO_ACHOU_30S',
        '10:00:00.000 MEDIDA sonda-ble achou ms=380',
      ]);
      expect(textos.first.style?.color, isNot(textos.last.style?.color));
    },
  );

  testWidgets('em andamento: botões de começar travados, PARAR solto', (
    tester,
  ) async {
    final a = await montar(tester, AndroidFalso());
    await doAndroid(tester, 'ocupado', true);
    expect(find.text('EM ANDAMENTO'), findsOneWidget);
    final antes = a.chamadas.length;
    await tester.tap(find.byKey(const Key('sonda-procurar')));
    await tester.tap(find.byKey(const Key('sonda-anunciar')));
    await tester.pumpAndSettle();
    expect(
      a.chamadas.length,
      antes,
      reason: 'nada pode sair com a sonda ocupada',
    );
    await tester.tap(find.byKey(const Key('sonda-parar')));
    await tester.pumpAndSettle();
    expect(a.chamadas.last.method, 'parar');
    await doAndroid(tester, 'ocupado', false);
    expect(find.text('PARADA'), findsOneWidget);
  });

  testWidgets('sem permissão: aviso, nada de inventário, botões travados', (
    tester,
  ) async {
    final a = await montar(tester, AndroidFalso(permite: false));
    expect(a.metodos, ['permissoes']);
    expect(find.byKey(const Key('sonda-sem-permissao')), findsOneWidget);
    await tester.tap(find.byKey(const Key('sonda-procurar')));
    await tester.tap(find.byKey(const Key('sonda-parar')));
    await tester.pumpAndSettle();
    expect(a.metodos, ['permissoes']);
    await tester.tap(find.byKey(const Key('sonda-tentar')));
    await tester.pumpAndSettle();
    expect(a.metodos, ['permissoes', 'permissoes']);
  });

  testWidgets('Android recusa: mostra o código, não derruba a tela', (
    tester,
  ) async {
    await montar(tester, AndroidFalso(recusarProcura: true));
    await tester.tap(find.byKey(const Key('sonda-procurar')));
    await tester.pumpAndSettle();
    expect(find.text('ERRO ARGUMENTO'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
