// Leitor de QR no computador: um quadro por vez, descarte enquanto o núcleo está
// ocupado, parada depois do achado e medida.

import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:kryptos/contatos.dart';
import 'package:kryptos/leitor_de_qr.dart';

Quadro quadro([int n = 0]) =>
    Quadro(brilho: Uint8List(4)..[0] = n, largura: 2, altura: 2, passo: 2);

void main() {
  test('com o núcleo ocupado, os quadros que chegam são descartados', () async {
    final pendentes = <Completer<ContatoLido?>>[];
    final achados = <ContatoLido>[];
    final leitor = LeitorDeQr(
      ler: (_) {
        final c = Completer<ContatoLido?>();
        pendentes.add(c);
        return c.future;
      },
      aoAchar: (c, _) => achados.add(c),
      aoRecusar: (_) {},
    );
    for (var i = 0; i < 5; i++) {
      leitor.receber(quadro(i));
    }
    expect(pendentes, hasLength(1), reason: 'mais de um quadro no núcleo');
    pendentes.single.complete(null);
    await Future<void>.delayed(Duration.zero);
    leitor.receber(quadro(9));
    expect(
      pendentes,
      hasLength(2),
      reason: 'o núcleo livre não pegou o próximo',
    );
    final m = leitor.medida();
    expect(m.lidos, 1);
    expect(m.descartados, 4);
    expect(achados, isEmpty);
  });

  test('achou: avisa uma vez, para e ignora o resto', () async {
    var chamadas = 0;
    final achados = <ContatoLido>[];
    MedidaDaLeitura? medida;
    final leitor = LeitorDeQr(
      ler: (_) async {
        chamadas++;
        return const ContatoLido(codigo: '2J2F-P63Y', novo: true);
      },
      aoAchar: (c, m) {
        achados.add(c);
        medida = m;
      },
      aoRecusar: (_) {},
    );
    leitor.receber(quadro());
    await Future<void>.delayed(Duration.zero);
    leitor.receber(quadro());
    leitor.receber(quadro());
    await Future<void>.delayed(Duration.zero);
    expect(achados.single.codigo, '2J2F-P63Y');
    expect(chamadas, 1, reason: 'continuou lendo depois de achar');
    expect(leitor.parado, isTrue);
    expect(medida!.lidos, 1);
  });

  test('recusa avisa e a leitura continua', () async {
    final recusas = <String>[];
    var vez = 0;
    final leitor = LeitorDeQr(
      ler: (_) async {
        vez++;
        if (vez < 3) throw const QrRecusado('QR_PROPRIO');
        return const ContatoLido(codigo: 'AAAA-BBBB', novo: false);
      },
      aoAchar: (_, _) {},
      aoRecusar: recusas.add,
    );
    for (var i = 0; i < 3; i++) {
      leitor.receber(quadro());
      await Future<void>.delayed(Duration.zero);
    }
    expect(recusas, ['QR_PROPRIO', 'QR_PROPRIO']);
    expect(leitor.parado, isTrue);
  });

  test('falha fora do previsto vai para a tela e não trava o leitor', () async {
    final recusas = <String>[];
    var vez = 0;
    final leitor = LeitorDeQr(
      ler: (_) async {
        vez++;
        if (vez == 1) throw StateError('quebrou');
        return null;
      },
      aoAchar: (_, _) {},
      aoRecusar: recusas.add,
    );
    leitor.receber(quadro());
    await Future<void>.delayed(Duration.zero);
    leitor.receber(quadro());
    await Future<void>.delayed(Duration.zero);
    expect(recusas, ['FALHA_StateError']);
    expect(vez, 2, reason: 'ficou ocupado depois da falha');
  });

  test(
    'parado não aceita quadro nem avisa resultado que chega depois',
    () async {
      final pendente = Completer<ContatoLido?>();
      final achados = <ContatoLido>[];
      final leitor = LeitorDeQr(
        ler: (_) => pendente.future,
        aoAchar: (c, _) => achados.add(c),
        aoRecusar: (_) {},
      );
      leitor.receber(quadro());
      leitor.parar();
      pendente.complete(const ContatoLido(codigo: 'AAAA-BBBB', novo: true));
      await Future<void>.delayed(Duration.zero);
      leitor.receber(quadro());
      expect(achados, isEmpty);
      expect(leitor.medida().descartados, 0);
    },
  );

  test('a linha da medida não leva o código do contato', () {
    const m = MedidaDaLeitura(
      tempo: Duration(milliseconds: 812),
      lidos: 12,
      descartados: 30,
      noNucleo: Duration(milliseconds: 7),
    );
    expect(
      m.linha('lido (novo)'),
      'Kryptos: MEDIDA qr lido (novo) em 812 ms, 12 quadros lidos, '
      '30 descartados, nucleo 7 ms por quadro',
    );
  });

  test('toda recusa do núcleo tem texto próprio', () {
    const codigos = [
      'QR_NAO_E_KRYPTOS',
      'QR_VERSAO_DESCONHECIDA',
      'QR_TAMANHO',
      'QR_FORMATO',
      'QR_NAO_CONFERE',
      'QR_CHAVE_INVALIDA',
      'QR_PROPRIO',
      'QR_CONFLITO',
      'SEM_IDENTIDADE',
      'QUADRO_INVALIDO',
      'QR_LEITOR',
    ];
    final generico = mensagemDaRecusa('XYZ');
    for (final c in codigos) {
      expect(mensagemDaRecusa(c), isNot(generico), reason: c);
    }
    expect(generico, contains('XYZ'));
  });
}
