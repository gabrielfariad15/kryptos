// O QR do cartão desenhado na tela: módulos escuros sobre o branco-osso, com a margem
// clara que o padrão pede. O tamanho do módulo é um número inteiro de pixels da tela,
// para a borda de cada módulo não sair borrada (a câmera do outro lado lê melhor).

import 'package:flutter/material.dart';

import '../contatos.dart';
import '../tema.dart';

class QrDesenhado extends StatelessWidget {
  const QrDesenhado({super.key, required this.grade});

  final GradeQr grade;

  /// Módulos de ponta a ponta, margem incluída.
  int get modulosNoLado => grade.lado + 2 * grade.margem;

  /// Lado do módulo em dp: o maior número inteiro de pixels que cabe em [largura].
  static double moduloEmDp(int modulos, double largura, double densidade) {
    final pixels = (largura * densidade / modulos).floorToDouble();
    return (pixels < 1 ? 1 : pixels) / densidade;
  }

  @override
  Widget build(BuildContext context) {
    final densidade = MediaQuery.devicePixelRatioOf(context);
    return LayoutBuilder(
      builder: (context, limites) {
        final modulo = moduloEmDp(modulosNoLado, limites.maxWidth, densidade);
        final lado = modulo * modulosNoLado;
        return Center(
          child: Semantics(
            label: 'QR com o seu cartão de contato',
            image: true,
            excludeSemantics: true,
            child: CustomPaint(
              key: const Key('qr-proprio'),
              size: Size.square(lado),
              painter: _PintorQr(grade, modulo),
            ),
          ),
        );
      },
    );
  }
}

class _PintorQr extends CustomPainter {
  _PintorQr(this.grade, this.modulo);

  final GradeQr grade;
  final double modulo;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRect(Offset.zero & size, Paint()..color = Cores.texto);
    final escuro = Paint()
      ..color = Cores.fundo
      ..isAntiAlias = false;
    final m = grade.margem;
    for (var y = 0; y < grade.lado; y++) {
      for (var x = 0; x < grade.lado; x++) {
        if (!grade.escuro(x, y)) continue;
        canvas.drawRect(
          Rect.fromLTWH((x + m) * modulo, (y + m) * modulo, modulo, modulo),
          escuro,
        );
      }
    }
  }

  @override
  bool shouldRepaint(_PintorQr antigo) =>
      antigo.grade != grade || antigo.modulo != modulo;
}
