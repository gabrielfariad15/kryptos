import 'package:flutter/material.dart';

import 'cofre.dart';
import 'contatos.dart';
import 'mensageiro.dart';
import 'plantao.dart';
import 'tema.dart';
import 'telas/central.dart';
import 'telas/desbloqueio.dart';
import 'telas/primeira_abertura.dart';

/// App sem dependência direta da ponte nem da câmera: recebe a versão do núcleo
/// pronta, a função que gera o código, o cofre, a agenda, a câmera e o mensageiro,
/// para os testes de tela rodarem no computador sem a biblioteca em Rust e sem o
/// plugin.
///
/// Sem cofre gravado: primeira abertura. Com cofre: desbloqueio. Depois de um dos
/// dois: a Central (modo 01), com o plantão ligado.
class KryptosApp extends StatelessWidget {
  const KryptosApp({
    super.key,
    required this.versaoNucleo,
    required this.gerarCodigo,
    required this.cofre,
    required this.agenda,
    required this.abrirCamera,
    required this.mensageiro,
  });

  final String versaoNucleo;
  final String Function() gerarCodigo;
  final Cofre cofre;
  final Agenda agenda;
  final AbrirCamera abrirCamera;
  final Mensageiro mensageiro;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Kryptos',
      debugShowCheckedModeBanner: false,
      theme: temaKryptos(),
      home: _Entrada(
        versaoNucleo: versaoNucleo,
        gerarCodigo: gerarCodigo,
        cofre: cofre,
        agenda: agenda,
        abrirCamera: abrirCamera,
        mensageiro: mensageiro,
      ),
    );
  }
}

class _Entrada extends StatefulWidget {
  const _Entrada({
    required this.versaoNucleo,
    required this.gerarCodigo,
    required this.cofre,
    required this.agenda,
    required this.abrirCamera,
    required this.mensageiro,
  });

  final String versaoNucleo;
  final String Function() gerarCodigo;
  final Cofre cofre;
  final Agenda agenda;
  final AbrirCamera abrirCamera;
  final Mensageiro mensageiro;

  @override
  State<_Entrada> createState() => _EntradaState();
}

class _EntradaState extends State<_Entrada> {
  late final bool _temCofre = widget.cofre.existe();
  String? _codigo;
  Plantao? _plantao;

  void _aberto(String codigo) {
    final plantao = Plantao(mensageiro: widget.mensageiro);
    setState(() {
      _codigo = codigo;
      _plantao = plantao;
    });
    plantao.iniciar();
  }

  @override
  void dispose() {
    _plantao?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final codigo = _codigo;
    final plantao = _plantao;
    if (codigo != null && plantao != null) {
      return Central(
        versaoNucleo: widget.versaoNucleo,
        codigo: codigo,
        agenda: widget.agenda,
        abrirCamera: widget.abrirCamera,
        plantao: plantao,
      );
    }
    if (_temCofre) {
      return Desbloqueio(
        versaoNucleo: widget.versaoNucleo,
        cofre: widget.cofre,
        aoAbrir: _aberto,
      );
    }
    return PrimeiraAbertura(
      versaoNucleo: widget.versaoNucleo,
      gerarCodigo: widget.gerarCodigo,
      cofre: widget.cofre,
      aoConcluir: _aberto,
    );
  }
}
