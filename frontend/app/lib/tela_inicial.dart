import 'package:flutter/material.dart';

import 'tema.dart';
import 'telas/primeira_abertura.dart';

/// App sem dependência direta da ponte: recebe a versão do núcleo pronta e a
/// função que gera o código, para os testes de tela rodarem no computador sem a
/// biblioteca em Rust.
class KryptosApp extends StatelessWidget {
  const KryptosApp({
    super.key,
    required this.versaoNucleo,
    required this.gerarCodigo,
  });

  final String versaoNucleo;
  final String Function() gerarCodigo;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Kryptos',
      debugShowCheckedModeBanner: false,
      theme: temaKryptos(),
      home: PrimeiraAbertura(
        versaoNucleo: versaoNucleo,
        gerarCodigo: gerarCodigo,
      ),
    );
  }
}
