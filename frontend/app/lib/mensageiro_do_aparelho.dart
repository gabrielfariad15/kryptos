// Mensageiro de verdade: a caixa selada (núcleo + Keystore) e o correio pelo relay.
// Sem relay (app montado sem endereço), só a caixa: ler e queimar o que já chegou.
//
// Nada de texto, carta ou código de contato em log (regra 8.2).

import 'dart:typed_data';

import 'caixa_do_aparelho.dart';
import 'correio.dart';
import 'mensageiro.dart';
import 'relay.dart';

class MensageiroDoAparelho implements Mensageiro {
  MensageiroDoAparelho({required this.caixa, this.correio});

  final CaixaDoAparelho caixa;
  final Correio? correio;

  @override
  bool get temRelay => correio != null;

  @override
  Future<void> preparar() async {
    await caixa.queimarLidas();
    await caixa.prepararChaves();
  }

  @override
  Future<void> publicarChaves() async {
    await correio?.publicarChaves();
  }

  @override
  Future<Recebidas> buscar() async {
    final c = correio;
    if (c == null) {
      return const Recebidas(seladas: 0, naoAbriram: 0, caixaCheia: false);
    }
    return c.buscar();
  }

  @override
  Future<List<Selada>> listar() async => [
    for (final m in await caixa.listar())
      Selada(gaveta: m.gaveta, lida: m.lida, remetente: m.remetente),
  ];

  @override
  Future<int> naoAbertas() => caixa.naoAbertas();

  @override
  Future<Aberta> ler(int gaveta) async {
    final m = await caixa.ler(gaveta);
    return Aberta(remetente: m.remetente, texto: m.texto);
  }

  @override
  Future<int> queimarVencidas() => caixa.queimarVencidas();

  @override
  Future<int> queimarLidas() => caixa.queimarLidas();

  @override
  Future<void> enviar(String codigo, Uint8List texto) async {
    final c = correio;
    if (c == null) throw const FalhaDoRelay('SEM_RELAY');
    await c.enviar(codigo, texto);
  }
}
