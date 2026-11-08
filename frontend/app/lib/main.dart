import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'agenda_do_aparelho.dart';
import 'caixa_do_aparelho.dart';
import 'camera_do_aparelho.dart';
import 'cofre_do_aparelho.dart';
import 'correio.dart';
import 'mensageiro_do_aparelho.dart';
import 'relay.dart';
import 'src/rust/api/nucleo.dart';
import 'src/rust/frb_generated.dart';
import 'tela_inicial.dart';

/// Endereço do relay, fixado na montagem do app:
///   flutter run --dart-define=KRYPTOS_RELAY=http://127.0.0.1:18080
/// Sem ele, o app só lê e queima o que já está na caixa. http só vale para a própria
/// máquina (relay de teste pelo adb reverse); fora disso, https (relay.dart).
const enderecoDoRelay = String.fromEnvironment('KRYPTOS_RELAY');

/// Só para filmar e capturar no teste: tira o FLAG_SECURE que o MainActivity liga.
///   flutter build apk --debug --dart-define=KRYPTOS_TELA_LIVRE=true
/// O Android recusa em app que não é de debug (MainActivity.kt), e o app mostra uma
/// faixa laranja em todas as telas.
const telaLivre = bool.fromEnvironment('KRYPTOS_TELA_LIVRE');

Future<void> main() async {
  // Carrega a biblioteca do núcleo (Rust) e pede ao Android a pasta do cofre.
  // Se falhar, o app para com o erro: sem núcleo ou sem pasta não há o que mostrar.
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  if (telaLivre) {
    // Se o Android recusar, o app para com o erro: melhor que fingir tela livre.
    await const MethodChannel('kryptos/tela').invokeMethod<void>('liberar');
  }
  final chave = ChaveDoAparelho();
  final pastas = await chave.pastas();
  final caixa = CaixaDoAparelho(chave: chave);
  final relay = enderecoDoRelay.isEmpty
      ? null
      : ClienteDoRelay(Uri.parse(enderecoDoRelay));
  runApp(
    KryptosApp(
      versaoNucleo: versaoNucleo(),
      gerarCodigo: gerarCodigoContato,
      cofre: CofreDoAparelho(pasta: pastas.semBackup, chave: chave),
      agenda: const AgendaDoAparelho(),
      abrirCamera: CameraDoAparelho.new,
      mensageiro: MensageiroDoAparelho(
        caixa: caixa,
        correio: relay == null ? null : Correio(relay: relay, caixa: caixa),
      ),
      telaLivre: telaLivre,
    ),
  );
}
