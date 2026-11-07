// Testes do novo contato por QR no computador, com agenda e câmera falsas
// (apoio.dart). O que só o aparelho prova (câmera de verdade, leitura de um QR na tela
// do outro celular) fica para o teste nos dois aparelhos.

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kryptos/contatos.dart';
import 'package:kryptos/plantao.dart';
import 'package:kryptos/telas/central.dart';
import 'package:kryptos/tema.dart';

import 'apoio.dart';

const codigoProprio = '2J2F-P63Y';
const codigoOutro = 'K7QX-M2ND';

Future<void> abrirCofre(
  WidgetTester tester, {
  required AgendaFalsa agenda,
  required FabricaDeCameras cameras,
  Aparelho aparelho = motorolaEdge40Neo,
}) async {
  tester.view.physicalSize = Size(aparelho.largura, aparelho.altura);
  tester.view.devicePixelRatio = aparelho.densidade;
  tester.view.padding = FakeViewPadding(
    top: aparelho.barraTopo,
    bottom: aparelho.barraBase,
  );
  tester.view.viewPadding = FakeViewPadding(
    top: aparelho.barraTopo,
    bottom: aparelho.barraBase,
  );
  addTearDown(tester.view.reset);
  final plantao = Plantao(mensageiro: MensageiroFalso());
  addTearDown(plantao.dispose);
  await tester.pumpWidget(
    MaterialApp(
      theme: temaKryptos(),
      home: Central(
        versaoNucleo: '9.8.7',
        codigo: codigoProprio,
        agenda: agenda,
        abrirCamera: cameras.call,
        plantao: plantao,
      ),
    ),
  );
}

Future<void> irParaNovoContato(WidgetTester tester) async {
  await tocarBotao(tester, 'NOVO CONTATO');
  await tester.pumpAndSettle();
}

Future<void> lerQr(WidgetTester tester) async {
  await tocarBotao(tester, 'LER');
  // abrir() e receberQuadros() da câmera falsa terminam em microtarefas.
  for (var i = 0; i < 3; i++) {
    await tester.pump();
  }
}

Future<void> mandarQuadro(WidgetTester tester, CameraFalsa camera) async {
  camera.mandar();
  for (var i = 0; i < 3; i++) {
    await tester.pump();
  }
}

ContatoLido contatoNovo() => const ContatoLido(codigo: codigoOutro, novo: true);

Future<void> _fluxoComRecusaELeitura(WidgetTester tester) async {
  var vez = 0;
  final agenda = AgendaFalsa()
    ..responder = (_) async {
      vez++;
      if (vez == 1) throw const QrRecusado('QR_CONFLITO');
      return contatoNovo();
    };
  final cameras = FabricaDeCameras();
  await abrirCofre(tester, agenda: agenda, cameras: cameras);
  await irParaNovoContato(tester);
  await lerQr(tester);
  await mandarQuadro(tester, cameras.ultima);
  await mandarQuadro(tester, cameras.ultima);
  expect(find.text('CONTATO ADICIONADO'), findsOneWidget);
}

void main() {
  setUpAll(carregarFontes);

  testWidgets('central sem contatos: lista vazia e NOVO CONTATO ligado', (
    tester,
  ) async {
    await abrirCofre(
      tester,
      agenda: AgendaFalsa(),
      cameras: FabricaDeCameras(),
    );
    expect(find.text('CANAIS (0)'), findsOneWidget);
    expect(acaoDoBotao(tester, 'NOVO CONTATO'), isNotNull);
  });

  testWidgets('a tela mostra o QR próprio e o código, com a câmera desligada', (
    tester,
  ) async {
    final cameras = FabricaDeCameras();
    await abrirCofre(tester, agenda: AgendaFalsa(), cameras: cameras);
    await irParaNovoContato(tester);
    expect(find.byKey(const Key('qr-proprio')), findsOneWidget);
    expect(
      tester.widget<Text>(find.byKey(const Key('codigo-proprio'))).data,
      codigoProprio,
    );
    expect(cameras.abertas, isEmpty, reason: 'câmera ligada sem pedir');
    expect(find.text('DEPOIS DA TROCA'), findsOneWidget);
  });

  testWidgets('o QR tem módulo de pixels inteiros e cabe na largura', (
    tester,
  ) async {
    for (final aparelho in [motorolaEdge40Neo, pequeno]) {
      await abrirCofre(
        tester,
        agenda: AgendaFalsa(),
        cameras: FabricaDeCameras(),
        aparelho: aparelho,
      );
      await irParaNovoContato(tester);
      final lado = tester.getSize(find.byKey(const Key('qr-proprio'))).width;
      // 37 módulos + 4 de margem de cada lado.
      final modulo = lado / 45;
      final pixels = modulo * aparelho.densidade;
      expect(
        pixels,
        closeTo(pixels.roundToDouble(), 1e-6),
        reason: '${aparelho.nome}: módulo de $pixels px',
      );
      expect(pixels, greaterThanOrEqualTo(4), reason: aparelho.nome);
      final largura = aparelho.largura / aparelho.densidade;
      expect(lado, lessThanOrEqualTo(largura - 36), reason: aparelho.nome);
      // O vizinho mais largo já não caberia: o QR é o maior possível.
      expect(
        (pixels + 1) * 45 / aparelho.densidade,
        greaterThan(largura - 36),
        reason: aparelho.nome,
      );
      await tester.pumpWidget(const SizedBox());
    }
  });

  testWidgets(
    'lê o QR: contato entra, a câmera fecha e a agenda mostra a chave conferida',
    (tester) async {
      final agenda = AgendaFalsa()..responder = (_) async => contatoNovo();
      final cameras = FabricaDeCameras();
      await abrirCofre(tester, agenda: agenda, cameras: cameras);
      await irParaNovoContato(tester);
      await lerQr(tester);
      final camera = cameras.ultima;
      expect(camera.aberta, isTrue);
      expect(camera.recebendo, isTrue);
      expect(find.byKey(const Key('previa-falsa')), findsOneWidget);

      await mandarQuadro(tester, camera);
      expect(find.text('CONTATO ADICIONADO'), findsOneWidget);
      expect(
        tester.widget<Text>(find.byKey(const Key('codigo-lido'))).data,
        codigoOutro,
      );
      expect(find.byKey(const Key('chave-conferida')), findsOneWidget);
      expect(camera.fechamentos, 1);
      expect(camera.aberta, isFalse);
      expect(find.byKey(const Key('previa-falsa')), findsNothing);

      await tester.tap(find.byKey(const Key('voltar')));
      await tester.pumpAndSettle();
      expect(find.text('CANAIS (1)'), findsOneWidget);
      expect(find.byKey(const Key('canal-$codigoOutro')), findsOneWidget);
      expect(find.text('CONVERSA · CHAVE CONFERIDA'), findsOneWidget);
    },
  );

  testWidgets('ler de novo quem já está na agenda avisa e não duplica', (
    tester,
  ) async {
    final agenda = AgendaFalsa()
      ..responder = (_) async =>
          const ContatoLido(codigo: codigoOutro, novo: false);
    final cameras = FabricaDeCameras();
    await abrirCofre(tester, agenda: agenda, cameras: cameras);
    await irParaNovoContato(tester);
    await lerQr(tester);
    await mandarQuadro(tester, cameras.ultima);
    expect(find.text('JÁ ESTAVA NA AGENDA'), findsOneWidget);
    expect(agenda.lista, isEmpty);
  });

  testWidgets('QR recusado avisa em laranja e a câmera continua lendo', (
    tester,
  ) async {
    var vez = 0;
    final agenda = AgendaFalsa()
      ..responder = (_) async {
        vez++;
        if (vez == 1) throw const QrRecusado('QR_PROPRIO');
        return contatoNovo();
      };
    final cameras = FabricaDeCameras();
    await abrirCofre(tester, agenda: agenda, cameras: cameras);
    await irParaNovoContato(tester);
    await lerQr(tester);
    final camera = cameras.ultima;
    await mandarQuadro(tester, camera);
    expect(textoDoAviso(tester), mensagemDaRecusa('QR_PROPRIO'));
    final aviso = tester.widget<Text>(find.byKey(const Key('aviso')));
    expect(aviso.style?.color, Cores.laranja);
    expect(camera.recebendo, isTrue, reason: 'parou de ler depois da recusa');
    expect(camera.fechamentos, 0);

    await mandarQuadro(tester, camera);
    expect(find.text('CONTATO ADICIONADO'), findsOneWidget);
  });

  testWidgets('sem permissão da câmera: volta ao QR com o aviso', (
    tester,
  ) async {
    final cameras = FabricaDeCameras()
      ..preparar = (c) => c.falhaAoAbrir = 'CameraAccessDenied';
    await abrirCofre(tester, agenda: AgendaFalsa(), cameras: cameras);
    await irParaNovoContato(tester);
    await lerQr(tester);
    expect(textoDoAviso(tester), mensagemDaCamera('CameraAccessDenied'));
    expect(find.byKey(const Key('qr-proprio')), findsOneWidget);
    expect(cameras.ultima.fechamentos, 1);
    expect(acaoDoBotao(tester, 'LER'), isNotNull, reason: 'sem nova tentativa');
  });

  testWidgets('CANCELAR desliga a câmera e mostra o QR de novo', (
    tester,
  ) async {
    final cameras = FabricaDeCameras();
    await abrirCofre(tester, agenda: AgendaFalsa(), cameras: cameras);
    await irParaNovoContato(tester);
    await lerQr(tester);
    await tocarBotao(tester, 'CANCELAR');
    await tester.pump();
    expect(cameras.ultima.fechamentos, 1);
    expect(cameras.ultima.recebendo, isFalse);
    expect(find.byKey(const Key('qr-proprio')), findsOneWidget);
  });

  testWidgets(
    'CANCELAR enquanto a câmera liga: ela fecha ao terminar de ligar',
    (tester) async {
      final segurar = Completer<void>();
      final cameras = FabricaDeCameras()
        ..preparar = (c) => c.segurarAbertura = segurar;
      await abrirCofre(tester, agenda: AgendaFalsa(), cameras: cameras);
      await irParaNovoContato(tester);
      await lerQr(tester);
      expect(find.text('ABRINDO A CÂMERA…'), findsOneWidget);
      await tocarBotao(tester, 'CANCELAR');
      segurar.complete();
      for (var i = 0; i < 3; i++) {
        await tester.pump();
      }
      final camera = cameras.ultima;
      expect(
        camera.recebendo,
        isFalse,
        reason: 'câmera ficou mandando quadros',
      );
      expect(camera.aberta, isFalse);
      expect(camera.fechamentos, greaterThanOrEqualTo(1));
      expect(find.byKey(const Key('previa-falsa')), findsNothing);
    },
  );

  testWidgets('voltar no meio da leitura desliga a câmera', (tester) async {
    final cameras = FabricaDeCameras();
    await abrirCofre(tester, agenda: AgendaFalsa(), cameras: cameras);
    await irParaNovoContato(tester);
    await lerQr(tester);
    await tester.tap(find.byKey(const Key('voltar')));
    await tester.pumpAndSettle();
    expect(cameras.ultima.fechamentos, 1);
    expect(find.text('MODO 01 · CENTRAL'), findsOneWidget);
  });

  testWidgets(
    'app no fundo desliga a câmera; o pedido de permissão por cima não',
    (tester) async {
      final cameras = FabricaDeCameras();
      await abrirCofre(tester, agenda: AgendaFalsa(), cameras: cameras);
      await irParaNovoContato(tester);
      await lerQr(tester);
      final camera = cameras.ultima;
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      await tester.pump();
      expect(camera.fechamentos, 0, reason: 'fechou com o app ainda na tela');
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.paused);
      await tester.pump();
      expect(camera.fechamentos, 1);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
      await tester.pump();
      expect(find.byKey(const Key('qr-proprio')), findsOneWidget);
      expect(cameras.abertas, hasLength(1), reason: 'religou sozinha');
    },
  );

  testWidgets('MOSTRAR MEU QR volta ao QR para o outro lado ler', (
    tester,
  ) async {
    final agenda = AgendaFalsa()..responder = (_) async => contatoNovo();
    final cameras = FabricaDeCameras();
    await abrirCofre(tester, agenda: agenda, cameras: cameras);
    await irParaNovoContato(tester);
    await lerQr(tester);
    await mandarQuadro(tester, cameras.ultima);
    await tocarBotao(tester, 'MOSTRAR');
    await tester.pump();
    expect(find.byKey(const Key('qr-proprio')), findsOneWidget);
    expect(acaoDoBotao(tester, 'LER'), isNotNull);
  });

  testWidgets('sem identidade na memória: sem QR e LER desligado', (
    tester,
  ) async {
    await abrirCofre(
      tester,
      agenda: AgendaFalsa(semIdentidade: true),
      cameras: FabricaDeCameras(),
    );
    await irParaNovoContato(tester);
    expect(find.byKey(const Key('qr-proprio')), findsNothing);
    expect(textoDoAviso(tester), mensagemDaRecusa('SEM_IDENTIDADE'));
    expect(acaoDoBotao(tester, 'LER'), isNull);
  });

  testWidgets('nenhum código de contato vai para o log', (tester) async {
    final log = <String>[];
    final antes = debugPrint;
    debugPrint = (m, {wrapWidth}) => log.add(m ?? '');
    // Volta dentro do teste: o framework confere que o debugPrint foi restaurado
    // antes do addTearDown rodar.
    try {
      await _fluxoComRecusaELeitura(tester);
    } finally {
      debugPrint = antes;
    }
    expect(log.where((l) => l.contains('MEDIDA qr lido')), hasLength(1));
    expect(log.where((l) => l.contains('QR_CONFLITO')), hasLength(1));
    for (final linha in log) {
      for (final codigo in [codigoProprio, codigoOutro]) {
        expect(linha, isNot(contains(codigo)));
        expect(linha, isNot(contains(codigo.replaceAll('-', ''))));
      }
    }
  });

  group('layout', () {
    for (final aparelho in [motorolaEdge40Neo, pequeno]) {
      testWidgets(
        '${aparelho.nome}: botão inteiro acima do rodapé nos três momentos',
        (tester) async {
          final agenda = AgendaFalsa()..responder = (_) async => contatoNovo();
          final cameras = FabricaDeCameras();
          await abrirCofre(
            tester,
            agenda: agenda,
            cameras: cameras,
            aparelho: aparelho,
          );
          await irParaNovoContato(tester);
          void conferir(String botao) {
            final acao = tester.getRect(find.byKey(Key('botao-$botao')));
            final rodape = tester.getRect(find.byKey(const Key('rodape')));
            expect(acao.bottom, lessThanOrEqualTo(rodape.top), reason: botao);
            expect(acao.height, 48, reason: botao);
            expect(tester.takeException(), isNull, reason: botao);
          }

          conferir('LER');
          await lerQr(tester);
          conferir('CANCELAR');
          await mandarQuadro(tester, cameras.ultima);
          conferir('MOSTRAR');
        },
      );
    }

    testWidgets('no motorola, QR, código e botão aparecem sem rolar', (
      tester,
    ) async {
      await abrirCofre(
        tester,
        agenda: AgendaFalsa(),
        cameras: FabricaDeCameras(),
      );
      await irParaNovoContato(tester);
      final altura = motorolaEdge40Neo.altura / motorolaEdge40Neo.densidade;
      final codigo = tester.getRect(find.byKey(const Key('codigo-proprio')));
      expect(codigo.bottom, lessThan(altura));
      final qr = tester.getRect(find.byKey(const Key('qr-proprio')));
      final ler = tester.getRect(find.byKey(const Key('botao-LER')));
      expect(qr.bottom, lessThan(codigo.top));
      expect(codigo.bottom, lessThan(ler.top));
    });

    testWidgets('fonte grande do sistema (160%) não estoura nada', (
      tester,
    ) async {
      tester.platformDispatcher.textScaleFactorTestValue = 1.6;
      addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
      final agenda = AgendaFalsa()..responder = (_) async => contatoNovo();
      final cameras = FabricaDeCameras();
      await abrirCofre(
        tester,
        agenda: agenda,
        cameras: cameras,
        aparelho: pequeno,
      );
      expect(tester.takeException(), isNull);
      await irParaNovoContato(tester);
      expect(tester.takeException(), isNull);
      await lerQr(tester);
      await mandarQuadro(tester, cameras.ultima);
      expect(tester.takeException(), isNull);
      await tester.tap(find.byKey(const Key('voltar')));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
    });
  });
}
