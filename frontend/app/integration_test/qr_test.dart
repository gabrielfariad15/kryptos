// Teste no aparelho do contato por QR, com o núcleo em Rust de verdade: prova que a
// grade do QR próprio, a leitura do quadro e a agenda atravessam a ponte dentro do
// Android, e mede o tempo de um quadro no processador do celular.
//
// O quadro é montado aqui, como a câmera entrega: só o plano de brilho, 640 × 480,
// com 64 bytes de sobra no fim de cada linha (o bytesPerRow do CameraX). A câmera de
// verdade e a leitura de um QR na tela do outro celular ficam para o teste nos dois
// aparelhos (ferramentas/medir-leitura-qr.ps1).
//
// Imprime uma linha MEDIDA, que a ferramentas/suite.ps1 -Aparelho mostra no resultado.
// Nada de código de contato, chave ou texto do QR.
//
// Rodar (com o celular no cabo):
//   flutter test integration_test

import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:kryptos/agenda_do_aparelho.dart';
import 'package:kryptos/contatos.dart';
import 'package:kryptos/src/rust/api/nucleo.dart';
import 'package:kryptos/src/rust/frb_generated.dart';

const largura = 640;
const altura = 480;
const sobra = 64;

/// Quadro cinza com o QR desenhado no meio, [px] pixels por módulo.
Quadro quadroCom(GradeQr? grade, {int px = 4}) {
  final passo = largura + sobra;
  final brilho = Uint8List(passo * altura);
  for (var y = 0; y < altura; y++) {
    brilho.fillRange(y * passo, y * passo + largura, 150);
  }
  if (grade != null) {
    final n = grade.lado + 2 * grade.margem;
    final x0 = (largura - n * px) ~/ 2;
    final y0 = (altura - n * px) ~/ 2;
    for (var y = 0; y < n * px; y++) {
      for (var x = 0; x < n * px; x++) {
        final mx = x ~/ px - grade.margem;
        final my = y ~/ px - grade.margem;
        brilho[(y0 + y) * passo + x0 + x] = grade.escuro(mx, my) ? 20 : 235;
      }
    }
  }
  return Quadro(brilho: brilho, largura: largura, altura: altura, passo: passo);
}

Future<Object?> recusa(Future<Object?> Function() f) async {
  try {
    await f();
    return null;
  } on QrRecusado catch (e) {
    return e.codigo;
  }
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  const agenda = AgendaDoAparelho();

  setUpAll(RustLib.init);
  setUp(esquecerIdentidade);
  tearDown(esquecerIdentidade);

  test('sem identidade não há QR próprio', () {
    expect(agenda.gradePropria(), isNull);
  });

  test('um aparelho lê o QR de outro e a chave entra conferida', () async {
    // Aparelho A mostra o QR.
    final codigoA = gerarCodigoContato();
    final gradeA = agenda.gradePropria()!;
    expect(gradeA.lado, 37);
    expect(gradeA.margem, 4);
    final quadroA = quadroCom(gradeA);

    // Aparelho B lê. (Os dois são o mesmo celular, um depois do outro.)
    esquecerIdentidade();
    final codigoB = gerarCodigoContato();
    expect(codigoB, isNot(codigoA));

    final relogio = Stopwatch()..start();
    final lido = await agenda.lerQuadro(quadroA);
    final msPrimeiro = relogio.elapsedMilliseconds;
    expect(lido?.codigo, codigoA);
    expect(lido?.novo, isTrue);

    // Mesmo QR de novo, 20 vezes: "já estava", e o tempo de um quadro com QR.
    relogio.reset();
    for (var i = 0; i < 20; i++) {
      final de = await agenda.lerQuadro(quadroA);
      expect(de?.novo, isFalse);
    }
    final msComQr = relogio.elapsedMilliseconds / 20;

    // Quadro sem QR, 20 vezes: o caso de quase todo quadro da câmera.
    final vazio = quadroCom(null);
    relogio.reset();
    for (var i = 0; i < 20; i++) {
      expect(await agenda.lerQuadro(vazio), isNull);
    }
    final msSemQr = relogio.elapsedMilliseconds / 20;

    final contatos = agenda.contatos();
    expect(contatos, hasLength(1));
    expect(contatos.single.codigo, codigoA);
    expect(contatos.single.conferidoPorQr, isTrue);

    // ignore: avoid_print
    print(
      'MEDIDA qr no aparelho: quadro ${largura}x$altura com QR '
      '${msComQr.toStringAsFixed(1)} ms, sem QR ${msSemQr.toStringAsFixed(1)} ms '
      '(primeiro $msPrimeiro ms, ponte incluida)',
    );
  });

  test('o próprio QR é recusado e não entra na agenda', () async {
    gerarCodigoContato();
    final proprio = quadroCom(agenda.gradePropria());
    expect(await recusa(() => agenda.lerQuadro(proprio)), 'QR_PROPRIO');
    expect(agenda.contatos(), isEmpty);
  });

  test(
    'quadro menor do que dizem largura, altura e passo é recusado',
    () async {
      gerarCodigoContato();
      final curto = Quadro(
        brilho: Uint8List(100),
        largura: largura,
        altura: altura,
        passo: largura + sobra,
      );
      expect(await recusa(() => agenda.lerQuadro(curto)), 'QUADRO_INVALIDO');
    },
  );

  test('passo ignorado entorta a imagem: o leitor não acha o QR', () async {
    // Prova de que o teste acima mede o caminho certo: com o passo errado, o mesmo
    // quadro deixa de ler.
    gerarCodigoContato();
    final q = quadroCom(agenda.gradePropria());
    esquecerIdentidade();
    gerarCodigoContato();
    final torto = Quadro(
      brilho: q.brilho,
      largura: largura,
      altura: altura,
      passo: largura,
    );
    expect(await agenda.lerQuadro(torto), isNull);
    expect(agenda.contatos(), isEmpty);
  });

  test('fechar o app esvazia a agenda', () async {
    gerarCodigoContato();
    final q = quadroCom(agenda.gradePropria());
    esquecerIdentidade();
    gerarCodigoContato();
    await agenda.lerQuadro(q);
    expect(agenda.contatos(), hasLength(1));
    esquecerIdentidade();
    expect(agenda.contatos(), isEmpty);
  });
}
