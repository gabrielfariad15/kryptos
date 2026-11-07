import 'package:flutter/material.dart';

import 'src/rust/api/nucleo.dart';
import 'src/rust/frb_generated.dart';
import 'tela_inicial.dart';

Future<void> main() async {
  // Carrega a biblioteca do núcleo (Rust). Se falhar, o app para com o erro:
  // sem núcleo não há o que mostrar.
  await RustLib.init();
  runApp(
    KryptosApp(versaoNucleo: versaoNucleo(), gerarCodigo: gerarCodigoContato),
  );
}
