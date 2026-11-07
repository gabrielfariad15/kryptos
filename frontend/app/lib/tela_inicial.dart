import 'package:flutter/material.dart';

import 'tema.dart';
import 'telas/primeira_abertura.dart';

/// App sem dependência direta da ponte: recebe a versão do núcleo pronta,
/// para os testes de tela rodarem no computador sem a biblioteca em Rust.
class KryptosApp extends StatelessWidget {
  const KryptosApp({super.key, required this.versaoNucleo});

  final String versaoNucleo;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Kryptos',
      debugShowCheckedModeBanner: false,
      theme: temaKryptos(),
      home: PrimeiraAbertura(versaoNucleo: versaoNucleo),
    );
  }
}
