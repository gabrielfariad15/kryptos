// Testes de tela da Central (modo 01) e da Conversa (modo 02) no computador, com
// mensageiro e relógio falsos (apoio.dart). O que só os dois celulares provam (carta
// indo de um aparelho ao outro, tela bloqueando print) fica para o teste no aparelho.

import 'dart:async';
import 'dart:convert';

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kryptos/relay.dart';
import 'package:kryptos/cofre.dart';
import 'package:kryptos/telas/conversa.dart';

import 'apoio.dart';

const amigo = 'K7QX-M2ND';
const outro = 'B08E-41D7';
const estranho = 'Z9Z9-Z9Z9';

Future<void> abrirConversa(WidgetTester tester, String codigo) async {
  await tester.tap(find.byKey(Key('canal-$codigo')));
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 400));
}

/// Começa a segurar a mensagem e espera o gesto ser aceito e o núcleo responder.
Future<TestGesture> segurar(WidgetTester tester, String id) async {
  final alvo = find.byKey(Key('segurar-$id'));
  await tester.ensureVisible(alvo);
  final gesto = await tester.startGesture(tester.getCenter(alvo));
  await tester.pump(kLongPressTimeout + const Duration(milliseconds: 20));
  for (var i = 0; i < 5; i++) {
    await tester.pump();
  }
  return gesto;
}

Future<void> soltar(WidgetTester tester, TestGesture gesto) async {
  await gesto.up();
  await tester.pump();
}

String? textoAberto(WidgetTester tester, String id) {
  final f = find.byKey(Key('texto-$id'));
  if (f.evaluate().isEmpty) return null;
  return tester.widget<Text>(f).data;
}

String meta(WidgetTester tester, String id) =>
    tester.widget<Text>(find.byKey(Key('meta-$id'))).data!;

void main() {
  setUpAll(carregarFontes);

  group('central', () {
    testWidgets('mostra as novas por canal, com 1 NOVA e 2 NOVAS', (
      tester,
    ) async {
      final m = MensageiroFalso()
        ..guardar(amigo, 'um')
        ..guardar(outro, 'dois')
        ..guardar(outro, 'três');
      await abrirCentral(
        tester,
        mensageiro: m,
        relogio: RelogioFalso(),
        agenda: agendaCom([amigo, outro]),
      );
      expect(find.text('MODO 01 · CENTRAL'), findsOneWidget);
      expect(find.text('CANAIS (2)'), findsOneWidget);
      expect(find.text('1 NOVA'), findsOneWidget);
      expect(find.text('2 NOVAS'), findsOneWidget);
      expect(
        find.descendant(
          of: find.byKey(const Key('canal-$amigo')),
          matching: find.text('1 NOVA'),
        ),
        findsOneWidget,
      );
      expect(find.text('RELAY LIGADO'), findsOneWidget);
      expect(
        tester.widget<Text>(find.byKey(const Key('codigo'))).data,
        codigoFalso,
      );
    });

    testWidgets(
      'remetente fora da agenda e caixa antiga aparecem como canais só de leitura',
      (tester) async {
        final m = MensageiroFalso()
          ..guardar(estranho, 'quem?')
          ..guardar('', 'antiga');
        await abrirCentral(
          tester,
          mensageiro: m,
          relogio: RelogioFalso(),
          agenda: agendaCom([amigo]),
        );
        expect(find.text('CANAIS (3)'), findsOneWidget);
        expect(find.text('FORA DA AGENDA · SÓ LEITURA'), findsOneWidget);
        expect(find.text('SEM REMETENTE'), findsOneWidget);
        expect(find.text('CAIXA ANTIGA · SÓ LEITURA'), findsOneWidget);
        await abrirConversa(tester, estranho);
        expect(find.byKey(const Key('campo-texto')), findsNothing);
        expect(find.byKey(const Key('sem-envio')), findsOneWidget);
      },
    );

    testWidgets('avisa as não abertas, a caixa cheia e a falha do relay', (
      tester,
    ) async {
      final m = MensageiroFalso()
        ..naoAbertasNoNucleo = 2
        ..falhaNaBusca = const FalhaDoRelay('REDE_SocketException');
      await abrirCentral(tester, mensageiro: m, relogio: RelogioFalso());
      expect(
        tester.widget<Text>(find.byKey(const Key('aviso-nao-abertas'))).data,
        startsWith('2 mensagens não puderam ser abertas'),
      );
      expect(find.byKey(const Key('aviso-relay')), findsOneWidget);
      expect(find.text('RELAY FORA'), findsOneWidget);
    });

    testWidgets('falha da caixa aparece com o código', (tester) async {
      final m = MensageiroFalso()..guardar(amigo, 'x');
      final relogio = RelogioFalso();
      final p = await abrirCentral(
        tester,
        mensageiro: m,
        relogio: relogio,
        agenda: agendaCom([amigo]),
      );
      m.falhaNaLeitura = const FalhaDoCofre('GAVETA_NAO_ABRE');
      expect(await p.abrir(p.seladas.single.gaveta), isNull);
      await tester.pump();
      expect(find.text('Falha na caixa (GAVETA_NAO_ABRE).'), findsOneWidget);
    });

    testWidgets('sem relay: avisa no cabeçalho e a conversa não tem campo', (
      tester,
    ) async {
      await abrirCentral(
        tester,
        mensageiro: MensageiroFalso(temRelay: false),
        relogio: RelogioFalso(),
        agenda: agendaCom([amigo]),
      );
      expect(find.text('SEM RELAY'), findsOneWidget);
      await abrirConversa(tester, amigo);
      expect(find.byKey(const Key('campo-texto')), findsNothing);
      expect(
        tester.widget<Text>(find.byKey(const Key('sem-envio'))).data,
        contains('sem relay'),
      );
    });

    testWidgets(
      'grupo e direto ficam apagados; conversa só liga depois de abrir um canal',
      (tester) async {
        await abrirCentral(
          tester,
          mensageiro: MensageiroFalso(),
          relogio: RelogioFalso(),
          agenda: agendaCom([amigo]),
        );
        InkWell modo(String m) =>
            tester.widget<InkWell>(find.byKey(Key('modo-$m')));
        expect(modo('GRUPO').onTap, isNull);
        expect(modo('DIRETO').onTap, isNull);
        expect(find.text('PRÓXIMA FASE'), findsNWidgets(2));
        expect(modo('CONVERSA').onTap, isNull);
        await abrirConversa(tester, amigo);
        await tester.tap(find.byKey(const Key('voltar-central')));
        await tester.pumpAndSettle();
        expect(modo('CONVERSA').onTap, isNotNull);
        await tester.tap(find.byKey(const Key('modo-CONVERSA')));
        await tester.pumpAndSettle();
        expect(find.text('MODO 02 · CONVERSA'), findsOneWidget);
        expect(
          tester.widget<Text>(find.byKey(const Key('codigo-conversa'))).data,
          amigo,
        );
      },
    );

    testWidgets('fonte de 160% num celular pequeno não estoura a central', (
      tester,
    ) async {
      tester.platformDispatcher.textScaleFactorTestValue = 1.6;
      addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
      await abrirCentral(
        tester,
        mensageiro: MensageiroFalso()
          ..guardar(amigo, 'a')
          ..guardar(estranho, 'b')
          ..naoAbertasNoNucleo = 1,
        relogio: RelogioFalso(),
        agenda: agendaCom([amigo, outro], naoConferidos: [outro]),
        aparelho: pequeno,
      );
      expect(tester.takeException(), isNull);
      await abrirConversa(tester, amigo);
      expect(tester.takeException(), isNull);
    });

    testWidgets('alvos de toque da central com 48 dp e rótulo', (tester) async {
      await abrirCentral(
        tester,
        mensageiro: MensageiroFalso()..guardar(amigo, 'a'),
        relogio: RelogioFalso(),
        agenda: agendaCom([amigo]),
      );
      final semantica = tester.ensureSemantics();
      await expectLater(tester, meetsGuideline(androidTapTargetGuideline));
      await expectLater(tester, meetsGuideline(labeledTapTargetGuideline));
      semantica.dispose();
    });
  });

  group('conversa', () {
    testWidgets(
      'fechada mostra 8 grupos; segurar abre, soltar fecha e zera o texto',
      (tester) async {
        final m = MensageiroFalso()
          ..guardar(amigo, 'curta')
          ..guardar(amigo, 'uma mensagem bem mais comprida que a outra');
        await abrirCentral(
          tester,
          mensageiro: m,
          relogio: RelogioFalso(),
          agenda: agendaCom([amigo]),
        );
        await abrirConversa(tester, amigo);
        expect(find.text('MODO 02 · CONVERSA'), findsOneWidget);
        expect(meta(tester, 'rx-0'), '$gruposFechados GRUPOS');
        expect(meta(tester, 'rx-1'), '$gruposFechados GRUPOS');
        expect(find.textContaining('curta'), findsNothing);
        expect(m.chamadas, isNot(contains('ler')), reason: 'abriu sem segurar');

        final gesto = await segurar(tester, 'rx-0');
        expect(textoAberto(tester, 'rx-0'), 'curta');
        expect(textoAberto(tester, 'rx-1'), isNull);
        expect(meta(tester, 'rx-0'), 'LIDA · QUEIMA EM 01:00');
        await soltar(tester, gesto);
        expect(textoAberto(tester, 'rx-0'), isNull);
        expect(find.textContaining('curta'), findsNothing);
        expect(m.entregues.single.every((b) => b == 0), isTrue);
      },
    );

    testWidgets('o sistema cancelar o toque no meio fecha o texto e zera', (
      tester,
    ) async {
      final m = MensageiroFalso()..guardar(amigo, 'cancelada');
      await abrirCentral(
        tester,
        mensageiro: m,
        relogio: RelogioFalso(),
        agenda: agendaCom([amigo]),
      );
      await abrirConversa(tester, amigo);
      final gesto = await segurar(tester, 'rx-0');
      expect(textoAberto(tester, 'rx-0'), 'cancelada');
      await gesto.cancel();
      await tester.pump();
      expect(textoAberto(tester, 'rx-0'), isNull);
      expect(m.entregues.single.every((b) => b == 0), isTrue);
    });

    testWidgets('um toque rápido não abre nem começa a queima', (tester) async {
      final m = MensageiroFalso()..guardar(amigo, 'não abre');
      await abrirCentral(
        tester,
        mensageiro: m,
        relogio: RelogioFalso(),
        agenda: agendaCom([amigo]),
      );
      await abrirConversa(tester, amigo);
      await tester.tap(find.byKey(const Key('segurar-rx-0')));
      await tester.pump(const Duration(seconds: 1));
      expect(m.chamadas, isNot(contains('ler')));
      expect(meta(tester, 'rx-0'), '$gruposFechados GRUPOS');
    });

    testWidgets(
      'soltar antes de o núcleo responder: nada aparece e o texto zera',
      (tester) async {
        final m = MensageiroFalso()
          ..guardar(amigo, 'atrasada')
          ..segurarLeitura = Completer<void>();
        await abrirCentral(
          tester,
          mensageiro: m,
          relogio: RelogioFalso(),
          agenda: agendaCom([amigo]),
        );
        await abrirConversa(tester, amigo);
        final gesto = await segurar(tester, 'rx-0');
        await soltar(tester, gesto);
        m.segurarLeitura!.complete();
        for (var i = 0; i < 5; i++) {
          await tester.pump();
        }
        expect(textoAberto(tester, 'rx-0'), isNull);
        expect(find.textContaining('atrasada'), findsNothing);
        expect(m.entregues.single.every((b) => b == 0), isTrue);
      },
    );

    testWidgets(
      'o texto some quando o app sai da frente com o dedo ainda nele',
      (tester) async {
        final m = MensageiroFalso()..guardar(amigo, 'na frente');
        await abrirCentral(
          tester,
          mensageiro: m,
          relogio: RelogioFalso(),
          agenda: agendaCom([amigo]),
        );
        await abrirConversa(tester, amigo);
        final gesto = await segurar(tester, 'rx-0');
        expect(textoAberto(tester, 'rx-0'), 'na frente');
        tester.binding.handleAppLifecycleStateChanged(
          AppLifecycleState.inactive,
        );
        await tester.pump();
        expect(textoAberto(tester, 'rx-0'), isNull);
        expect(m.entregues.single.every((b) => b == 0), isTrue);
        await gesto.up();
        tester.binding.handleAppLifecycleStateChanged(
          AppLifecycleState.resumed,
        );
        await tester.pump();
      },
    );

    testWidgets('a lida conta o minuto na tela e some quando queima', (
      tester,
    ) async {
      final relogio = RelogioFalso();
      final m = MensageiroFalso()
        ..guardar(amigo, 'queima')
        ..vencidas = true;
      await abrirCentral(
        tester,
        mensageiro: m,
        relogio: relogio,
        agenda: agendaCom([amigo]),
      );
      await abrirConversa(tester, amigo);
      await soltar(tester, await segurar(tester, 'rx-0'));
      await passar(tester, relogio, const Duration(seconds: 18));
      expect(meta(tester, 'rx-0'), 'LIDA · QUEIMA EM 00:42');
      await passar(tester, relogio, const Duration(seconds: 42));
      expect(find.byKey(const Key('msg-rx-0')), findsNothing);
      expect(find.byKey(const Key('conversa-vazia')), findsOneWidget);
    });

    testWidgets(
      'CIFRAR manda, limpa o campo e a enviada fica 60 s, fechada até segurar',
      (tester) async {
        final relogio = RelogioFalso();
        final m = MensageiroFalso();
        await abrirCentral(
          tester,
          mensageiro: m,
          relogio: relogio,
          agenda: agendaCom([amigo]),
        );
        await abrirConversa(tester, amigo);
        expect(acaoDoBotao(tester, 'CIFRAR'), isNull, reason: 'campo vazio');
        await tester.enterText(find.byKey(const Key('campo-texto')), 'olá, ç');
        await tester.pump();
        await tocarBotao(tester, 'CIFRAR');
        for (var i = 0; i < 5; i++) {
          await tester.pump();
        }
        expect(m.enviadas.single.$1, amigo);
        expect(m.enviadas.single.$2, utf8.encode('olá, ç'));
        expect(
          tester
              .widget<TextField>(find.byKey(const Key('campo-texto')))
              .controller!
              .text,
          isEmpty,
        );
        expect(meta(tester, 'tx-1'), 'ENVIADA · QUEIMA EM 01:00');
        expect(find.textContaining('olá'), findsNothing);
        final gesto = await segurar(tester, 'tx-1');
        expect(textoAberto(tester, 'tx-1'), 'olá, ç');
        await soltar(tester, gesto);
        expect(textoAberto(tester, 'tx-1'), isNull);
        await passar(tester, relogio, const Duration(seconds: 60));
        expect(find.byKey(const Key('msg-tx-1')), findsNothing);
      },
    );

    testWidgets('envio que falha avisa e deixa o texto no campo', (
      tester,
    ) async {
      final m = MensageiroFalso()
        ..falhaNoEnvio = const FalhaDoRelay('SEM_CHAVE');
      await abrirCentral(
        tester,
        mensageiro: m,
        relogio: RelogioFalso(),
        agenda: agendaCom([amigo]),
      );
      await abrirConversa(tester, amigo);
      await tester.enterText(find.byKey(const Key('campo-texto')), 'tenta');
      await tester.pump();
      await tocarBotao(tester, 'CIFRAR');
      for (var i = 0; i < 5; i++) {
        await tester.pump();
      }
      expect(
        tester.widget<Text>(find.byKey(const Key('aviso-envio'))).data,
        mensagemDoEnvio('SEM_CHAVE'),
      );
      expect(
        tester
            .widget<TextField>(find.byKey(const Key('campo-texto')))
            .controller!
            .text,
        'tenta',
      );
      expect(find.byKey(const Key('msg-tx-1')), findsNothing);
    });

    testWidgets('o teclado não aprende nem sugere o texto em claro', (
      tester,
    ) async {
      await abrirCentral(
        tester,
        mensageiro: MensageiroFalso(),
        relogio: RelogioFalso(),
        agenda: agendaCom([amigo]),
      );
      await abrirConversa(tester, amigo);
      final campo = tester.widget<TextField>(
        find.byKey(const Key('campo-texto')),
      );
      expect(campo.enableIMEPersonalizedLearning, isFalse);
      expect(campo.enableSuggestions, isFalse);
      expect(campo.autocorrect, isFalse);
    });

    testWidgets(
      'mensagens recebidas e enviadas ficam na ordem em que aconteceram',
      (tester) async {
        final relogio = RelogioFalso();
        final m = MensageiroFalso()..guardar(amigo, 'primeira');
        await abrirCentral(
          tester,
          mensageiro: m,
          relogio: relogio,
          agenda: agendaCom([amigo]),
        );
        await abrirConversa(tester, amigo);
        await passar(tester, relogio, const Duration(seconds: 2));
        await tester.enterText(find.byKey(const Key('campo-texto')), 'segunda');
        await tester.pump();
        await tocarBotao(tester, 'CIFRAR');
        for (var i = 0; i < 5; i++) {
          await tester.pump();
        }
        m.noRelay.add((amigo, 'terceira'));
        await passar(tester, relogio, const Duration(seconds: 8));
        final y0 = tester.getTopLeft(find.byKey(const Key('msg-rx-0'))).dy;
        final y1 = tester.getTopLeft(find.byKey(const Key('msg-tx-1'))).dy;
        final y2 = tester.getTopLeft(find.byKey(const Key('msg-rx-1'))).dy;
        expect(y0, lessThan(y1));
        expect(y1, lessThan(y2));
        expect(find.text('MSG 03 · RX 20:14'), findsOneWidget);
      },
    );
  });

  test(
    'aviso de envio para cada falha conhecida, sem repetir o código cru',
    () {
      expect(mensagemDoEnvio('TEXTO_GRANDE'), contains('65.536'));
      expect(
        mensagemDoEnvio('REDE_SocketException'),
        startsWith('Sem conexão'),
      );
      expect(mensagemDoEnvio('XYZ'), contains('XYZ'));
    },
  );
}
