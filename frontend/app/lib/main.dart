import 'package:flutter/material.dart';

import 'agenda_do_aparelho.dart';
import 'camera_do_aparelho.dart';
import 'cofre_do_aparelho.dart';
import 'src/rust/api/nucleo.dart';
import 'src/rust/frb_generated.dart';
import 'tela_inicial.dart';

Future<void> main() async {
  // Carrega a biblioteca do núcleo (Rust) e pede ao Android a pasta do cofre.
  // Se falhar, o app para com o erro: sem núcleo ou sem pasta não há o que mostrar.
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  final chave = ChaveDoAparelho();
  final pastas = await chave.pastas();
  runApp(
    KryptosApp(
      versaoNucleo: versaoNucleo(),
      gerarCodigo: gerarCodigoContato,
      cofre: CofreDoAparelho(pasta: pastas.semBackup, chave: chave),
      agenda: const AgendaDoAparelho(),
      abrirCamera: CameraDoAparelho.new,
    ),
  );
}
