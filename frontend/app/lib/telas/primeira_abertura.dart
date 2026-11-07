// Tela de primeira abertura: gerar o código de contato e criar o PIN.
//
// GERAR CÓDIGO pede ao núcleo a identidade (vodozemac) e mostra o código de
// contato que sai da chave pública. A chave fica no núcleo; aqui só chega o código,
// que não vai para log (regra 8.2).
//
// CONTINUAR liga com o código gerado e os seis dígitos. O primeiro toque pede o PIN
// de novo; o segundo, com os dois iguais, grava a identidade no cofre (Argon2id no
// núcleo + chave do Keystore) e passa para a tela do cofre aberto. Os dígitos ficam
// só na memória deste widget, zerados ao sair, e não vão para log, disco nem tela.

import 'package:flutter/material.dart';

import '../cofre.dart';
import '../tema.dart';
import 'componentes.dart';

enum _Etapa { pin, confirmacao, gravando }

class PrimeiraAbertura extends StatefulWidget {
  const PrimeiraAbertura({
    super.key,
    required this.versaoNucleo,
    required this.gerarCodigo,
    required this.cofre,
    required this.aoConcluir,
  });

  /// Versão informada pelo núcleo em Rust, pela ponte.
  final String versaoNucleo;

  /// Gera a identidade no núcleo e devolve o código (XXXX-XXXX). Vem de fora para
  /// os testes de tela rodarem no computador sem a biblioteca em Rust.
  final String Function() gerarCodigo;

  final Cofre cofre;

  /// Chamado com o código depois que a identidade foi gravada.
  final void Function(String codigo) aoConcluir;

  @override
  State<PrimeiraAbertura> createState() => _PrimeiraAberturaState();
}

class _PrimeiraAberturaState extends State<PrimeiraAbertura> {
  final List<int> _pin = [];
  final List<int> _primeiro = [];
  _Etapa _etapa = _Etapa.pin;
  String? _codigo;
  bool _falhou = false;
  String? _aviso;

  void _gerar() {
    if (_codigo != null) return;
    try {
      final codigo = widget.gerarCodigo();
      setState(() {
        _codigo = codigo;
        _falhou = false;
      });
    } catch (e) {
      // Só o tipo do erro: o texto completo fica fora do log por cautela (regra 8.2).
      debugPrint(
        'Kryptos: falha ao gerar o código de contato (${e.runtimeType})',
      );
      setState(() => _falhou = true);
    }
  }

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

  bool get _podeContinuar =>
      _codigo != null && _pin.length == tamanhoPin && _etapa != _Etapa.gravando;

  Future<void> _continuar() async {
    if (!_podeContinuar) return;
    if (_etapa == _Etapa.pin) {
      setState(() {
        _primeiro
          ..clear()
          ..addAll(_pin);
        _limpar(_pin);
        _etapa = _Etapa.confirmacao;
      });
      return;
    }
    if (!_iguais(_primeiro, _pin)) {
      setState(() {
        _limpar(_pin);
        _limpar(_primeiro);
        _etapa = _Etapa.pin;
        _aviso = 'Os dois PINs não conferem. Digite de novo.';
      });
      return;
    }
    setState(() => _etapa = _Etapa.gravando);
    final pin = List<int>.of(_pin);
    try {
      await widget.cofre.criar(pin);
      if (!mounted) return;
      final codigo = _codigo!;
      _limpar(_pin);
      _limpar(_primeiro);
      widget.aoConcluir(codigo);
    } catch (e) {
      debugPrint(
        'Kryptos: falha ao gravar a identidade (${e is FalhaDoCofre ? e.codigo : e.runtimeType})',
      );
      if (!mounted) return;
      setState(() {
        _etapa = _Etapa.confirmacao;
        _aviso = e is FalhaDoCofre
            ? 'Não foi possível gravar a identidade (${e.codigo}). Toque em CONTINUAR de novo.'
            : 'Não foi possível gravar a identidade. Toque em CONTINUAR de novo.';
      });
    } finally {
      _limpar(pin);
    }
  }

  static bool _iguais(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    var dif = 0;
    for (var i = 0; i < a.length; i++) {
      dif |= a[i] ^ b[i];
    }
    return dif == 0;
  }

  /// Sobrescreve antes de esvaziar: a lista não deixa os dígitos para trás.
  static void _limpar(List<int> l) {
    l.fillRange(0, l.length, 0);
    l.clear();
  }

  @override
  void dispose() {
    _limpar(_pin);
    _limpar(_primeiro);
    _codigo = null;
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final confirmando = _etapa != _Etapa.pin;
    final gravando = _etapa == _Etapa.gravando;
    return MoldeDeEntrada(
      titulo: 'PRIMEIRA ABERTURA',
      versaoNucleo: widget.versaoNucleo,
      conteudo: [
        const Text('01 · SEU CÓDIGO', style: estiloRotulo),
        const SizedBox(height: 10),
        PainelCodigo(
          codigo: _codigo,
          legenda: 'Criado neste aparelho, a partir da sua chave.',
          rodape: [
            if (_falhou) ...[
              const SizedBox(height: 6),
              const Text(
                'Não foi possível gerar o código. Toque de novo.',
                key: Key('falha-codigo'),
                style: estiloAviso,
              ),
            ],
            const SizedBox(height: 10),
            // Depois de gerado, o botão desliga: a identidade é uma só por aparelho.
            Botao(
              rotulo: 'GERAR CÓDIGO',
              aoTocar: _codigo != null ? null : _gerar,
            ),
          ],
        ),
        const SizedBox(height: 16),
        Text(
          confirmando ? '02 · REPITA O PIN' : '02 · PIN DE ACESSO',
          key: const Key('rotulo-pin'),
          style: estiloRotulo,
        ),
        const SizedBox(height: 10),
        CasasPin(preenchidas: _pin.length),
        const SizedBox(height: 8),
        if (_aviso != null)
          Aviso(_aviso!)
        else
          Text(
            confirmando
                ? 'Digite o mesmo PIN de novo para conferir.'
                : 'Seis dígitos. Abre o Kryptos neste aparelho.',
            style: estiloTextoCorrido,
          ),
        const SizedBox(height: 10),
        Teclado(aoDigitar: _digitar, aoApagar: _apagar, ligado: !gravando),
        const SizedBox(height: 14),
        const _AvisoSemCopia(),
      ],
      // CONTINUAR fora da rolagem: fica sempre inteiro, em qualquer altura de tela.
      acao: Botao(
        nome: 'CONTINUAR',
        rotulo: gravando ? 'GRAVANDO…' : 'CONTINUAR',
        aoTocar: _podeContinuar ? _continuar : null,
      ),
    );
  }
}

class _AvisoSemCopia extends StatelessWidget {
  const _AvisoSemCopia();

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(border: Border.all(color: Cores.laranja)),
      child: const Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'SEM CÓPIA FORA DAQUI',
            style: TextStyle(
              fontSize: 11,
              fontWeight: FontWeight.w700,
              letterSpacing: 1,
              color: Cores.laranja,
            ),
          ),
          SizedBox(height: 6),
          Text(
            'Não há número, e-mail nem conta em servidor. A chave e o PIN ficam só neste aparelho. Esqueceu o PIN, perdeu a identidade.',
            style: estiloTextoCorrido,
          ),
        ],
      ),
    );
  }
}
