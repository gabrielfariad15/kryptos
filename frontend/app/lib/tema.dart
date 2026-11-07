// Visual aprovado em 06/11/2026 (linha "KRYPTOS unificado").
// Cores e fonte: documentacao/DECISOES-FASE-0.md, seção 4.

import 'package:flutter/material.dart';

abstract final class Cores {
  static const fundo = Color(0xFF0D0B07);
  static const painel = Color(0xFF17130B);
  static const linha = Color(0xFF3A2E14);
  static const linhaTracejada = Color(0xFF5C4A1F);
  static const texto = Color(0xFFEDE6D6);
  static const textoCorrido = Color(0xFFC9BFA8);
  static const secundario = Color(0xFFA08D66);

  /// Estrutura: botões, cabeçalhos, seleção.
  static const ambar = Color(0xFFFFB000);

  /// Só alerta e destruição.
  static const laranja = Color(0xFFFF6A13);
}

const fonte = 'IBMPlexMono';

/// Rótulo pequeno espaçado (ex.: "CANAIS", "SEU CÓDIGO").
const estiloRotulo = TextStyle(
  fontFamily: fonte,
  fontSize: 11,
  letterSpacing: 2,
  color: Cores.secundario,
);

ThemeData temaKryptos() {
  return ThemeData(
    useMaterial3: true,
    brightness: Brightness.dark,
    fontFamily: fonte,
    scaffoldBackgroundColor: Cores.fundo,
    colorScheme: const ColorScheme.dark(
      surface: Cores.fundo,
      primary: Cores.ambar,
      onPrimary: Cores.fundo,
      error: Cores.laranja,
      onSurface: Cores.texto,
    ),
    splashFactory: NoSplash.splashFactory,
  );
}
