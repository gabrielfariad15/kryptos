// Agenda de verdade: o núcleo em Rust, pela ponte. A agenda fica só na memória do
// núcleo; aqui chegam o código de contato e a forma de conferência, nunca a chave
// nem o texto do QR.

import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';

import 'contatos.dart';
import 'src/rust/api/nucleo.dart' as nucleo;

class AgendaDoAparelho implements Agenda {
  const AgendaDoAparelho();

  @override
  GradeQr? gradePropria() {
    final g = _traduzir(nucleo.gradePropria);
    if (g == null) return null;
    return GradeQr(lado: g.lado, margem: g.margem, modulos: g.modulos);
  }

  @override
  Future<ContatoLido?> lerQuadro(Quadro quadro) async {
    try {
      final c = await nucleo.adicionarContatoDoQuadro(
        brilho: quadro.brilho,
        largura: quadro.largura,
        altura: quadro.altura,
        passo: quadro.passo,
      );
      if (c == null) return null;
      return ContatoLido(codigo: c.codigo, novo: c.novo);
    } on AnyhowException catch (e) {
      throw QrRecusado(_codigo(e));
    }
  }

  @override
  List<ContatoNaAgenda> contatos() => [
    for (final c in nucleo.contatos())
      ContatoNaAgenda(codigo: c.codigo, conferidoPorQr: c.conferidoPorQr),
  ];

  static T _traduzir<T>(T Function() f) {
    try {
      return f();
    } on AnyhowException catch (e) {
      throw QrRecusado(_codigo(e));
    }
  }

  /// O núcleo manda só o código fixo.
  static String _codigo(AnyhowException e) =>
      e.message.trim().split(RegExp(r'\s')).first;
}
