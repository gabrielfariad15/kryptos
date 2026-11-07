// Tela de desbloqueio: o cofre já existe; o PIN abre a identidade gravada.
//
// ABRIR liga com os seis dígitos. O núcleo registra a tentativa antes de conferir,
// cumpre a espera depois de 5 erradas seguidas e só então estica o PIN.
// PIN errado limpa as casas e avisa. Os dígitos ficam só na memória deste widget.

import 'package:flutter/material.dart';

import '../cofre.dart';
import '../tema.dart';
import 'componentes.dart';

class Desbloqueio extends StatefulWidget {
  const Desbloqueio({
    super.key,
    required this.versaoNucleo,
    required this.cofre,
    required this.aoAbrir,
  });

  final String versaoNucleo;
  final Cofre cofre;

  /// Chamado com o código de contato da identidade aberta.
  final void Function(String codigo) aoAbrir;

  @override
  State<Desbloqueio> createState() => _DesbloqueioState();
}

class _DesbloqueioState extends State<Desbloqueio> {
  final List<int> _pin = [];
  bool _conferindo = false;
  String? _aviso;

  void _digitar(int d) {
    if (_pin.length >= tamanhoPin) return;
    setState(() {
      _pin.add(d);
      _aviso = null;
    });
  }

  void _apagar() {
    if (_pin.isEmpty) return;
    setState(() => _pin.removeLast());
  }

  bool get _podeAbrir => _pin.length == tamanhoPin && !_conferindo;

  int _espera() {
    try {
      return widget.cofre.esperaSegundos();
    } catch (e) {
      debugPrint(
        'Kryptos: falha ao ler a espera (${e is FalhaDoCofre ? e.codigo : e.runtimeType})',
      );
      return 0;
    }
  }

  Future<void> _abrir() async {
    if (!_podeAbrir) return;
    final espera = _espera();
    setState(() {
      _conferindo = true;
      _aviso = espera > 0
          ? 'Muitas tentativas erradas. Esta conferência espera $espera s; não feche o app.'
          : null;
    });
    final pin = List<int>.of(_pin);
    try {
      final codigo = await widget.cofre.abrir(pin);
      if (!mounted) return;
      _limpar(_pin);
      widget.aoAbrir(codigo);
    } on PinIncorreto {
      if (!mounted) return;
      setState(() {
        _limpar(_pin);
        _conferindo = false;
        _aviso = 'PIN incorreto.';
      });
    } catch (e) {
      final codigo = e is FalhaDoCofre ? e.codigo : '${e.runtimeType}';
      debugPrint('Kryptos: falha ao abrir o cofre ($codigo)');
      if (!mounted) return;
      setState(() {
        _limpar(_pin);
        _conferindo = false;
        _aviso = codigo == 'CHAVE_AUSENTE'
            ? 'A chave deste aparelho não existe mais. A identidade gravada não pode ser aberta.'
            : 'Não foi possível abrir o cofre ($codigo).';
      });
    } finally {
      _limpar(pin);
    }
  }

  static void _limpar(List<int> l) {
    l.fillRange(0, l.length, 0);
    l.clear();
  }

  @override
  void dispose() {
    _limpar(_pin);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return MoldeDeEntrada(
      titulo: 'DESBLOQUEIO',
      versaoNucleo: widget.versaoNucleo,
      conteudo: [
        const Text('01 · PIN DE ACESSO', style: estiloRotulo),
        const SizedBox(height: 10),
        CasasPin(preenchidas: _pin.length),
        const SizedBox(height: 8),
        if (_aviso != null)
          Aviso(_aviso!)
        else
          const Text(
            'O PIN criado na primeira abertura deste aparelho.',
            style: estiloTextoCorrido,
          ),
        const SizedBox(height: 10),
        Teclado(aoDigitar: _digitar, aoApagar: _apagar, ligado: !_conferindo),
      ],
      acao: Botao(
        nome: 'ABRIR',
        rotulo: _conferindo ? 'CONFERINDO…' : 'ABRIR',
        aoTocar: _podeAbrir ? _abrir : null,
      ),
    );
  }
}
