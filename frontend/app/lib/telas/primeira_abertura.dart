// Tela de primeira abertura: gerar o código de contato e criar o PIN.
//
// Nesta etapa é só visual (decidido em 06/11/2026). O botão GERAR CÓDIGO e o
// CONTINUAR ficam desligados até o núcleo ganhar a identidade (vodozemac) e o
// PIN real (derivação de chave + Keystore), num assunto próprio.
// O teclado do PIN funciona para conferir o visual; os dígitos ficam só na
// memória deste widget e não vão para log, disco nem para a tela (regra 8.2).

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../tema.dart';

const tamanhoPin = 6;

/// Barras do sistema: fundo do app aparecendo atrás, ícones claros, sem véu.
const estiloBarras = SystemUiOverlayStyle(
  statusBarColor: Colors.transparent,
  statusBarIconBrightness: Brightness.light,
  statusBarBrightness: Brightness.dark,
  systemNavigationBarColor: Colors.transparent,
  systemNavigationBarIconBrightness: Brightness.light,
  systemNavigationBarContrastEnforced: false,
);

class PrimeiraAbertura extends StatefulWidget {
  const PrimeiraAbertura({super.key, required this.versaoNucleo});

  /// Versão informada pelo núcleo em Rust, pela ponte.
  final String versaoNucleo;

  @override
  State<PrimeiraAbertura> createState() => _PrimeiraAberturaState();
}

class _PrimeiraAberturaState extends State<PrimeiraAbertura> {
  final List<int> _pin = [];

  void _digitar(int d) {
    if (_pin.length >= tamanhoPin) return;
    setState(() => _pin.add(d));
  }

  void _apagar() {
    if (_pin.isEmpty) return;
    setState(() => _pin.removeLast());
  }

  @override
  void dispose() {
    _pin.clear();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // Android 15+ desenha o app atrás das barras do sistema. Sem isto, a barra de
    // 3 botões ganha um véu claro do sistema por cima do fundo escuro.
    return AnnotatedRegion<SystemUiOverlayStyle>(
      value: estiloBarras,
      child: Scaffold(
        body: SafeArea(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              const _Cabecalho(),
              Expanded(
                child: SingleChildScrollView(
                  padding: const EdgeInsets.fromLTRB(18, 14, 18, 12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      const Text('01 · SEU CÓDIGO', style: estiloRotulo),
                      const SizedBox(height: 10),
                      const _PainelCodigo(),
                      const SizedBox(height: 16),
                      const Text('02 · PIN DE ACESSO', style: estiloRotulo),
                      const SizedBox(height: 10),
                      _CasasPin(preenchidas: _pin.length),
                      const SizedBox(height: 8),
                      const Text(
                        'Seis dígitos. Abre o Kryptos neste aparelho.',
                        style: TextStyle(
                          fontSize: 11,
                          height: 1.6,
                          color: Cores.textoCorrido,
                        ),
                      ),
                      const SizedBox(height: 10),
                      _Teclado(aoDigitar: _digitar, aoApagar: _apagar),
                      const SizedBox(height: 14),
                      const _Aviso(),
                    ],
                  ),
                ),
              ),
              // CONTINUAR fora da rolagem: fica sempre inteiro, em qualquer altura de tela.
              const Padding(
                padding: EdgeInsets.fromLTRB(18, 10, 18, 10),
                child: _Botao(rotulo: 'CONTINUAR', aoTocar: null),
              ),
              _Rodape(versaoNucleo: widget.versaoNucleo),
            ],
          ),
        ),
      ),
    );
  }
}

class _Cabecalho extends StatelessWidget {
  const _Cabecalho();

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 14),
      decoration: const BoxDecoration(
        border: Border(bottom: BorderSide(color: Cores.ambar, width: 2)),
      ),
      child: const Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // Linha de base comum, como no visual aprovado. O rótulo da direita é
          // Expanded: com fonte grande do sistema ele quebra a linha em vez de estourar.
          Row(
            crossAxisAlignment: CrossAxisAlignment.baseline,
            textBaseline: TextBaseline.alphabetic,
            children: [
              Text(
                'KRYPTOS',
                style: TextStyle(
                  fontSize: 22,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 6,
                  color: Cores.ambar,
                ),
              ),
              SizedBox(width: 12),
              Expanded(
                child: Text(
                  'PRIMEIRA ABERTURA',
                  textAlign: TextAlign.right,
                  style: estiloRotulo,
                ),
              ),
            ],
          ),
          SizedBox(height: 8),
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Text(
                  'SEM NÚMERO · SEM E-MAIL',
                  style: TextStyle(
                    fontSize: 11,
                    letterSpacing: 1,
                    color: Cores.secundario,
                  ),
                ),
              ),
              SizedBox(width: 12),
              Text(
                'SEM CADASTRO',
                style: TextStyle(
                  fontSize: 11,
                  letterSpacing: 1,
                  color: Cores.secundario,
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _PainelCodigo extends StatelessWidget {
  const _PainelCodigo();

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: Cores.painel,
        border: Border.all(color: Cores.linha),
      ),
      child: const Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text('CÓDIGO DE CONTATO', style: estiloRotulo),
          SizedBox(height: 6),
          Text(
            '····-····',
            key: Key('codigo'),
            style: TextStyle(
              fontSize: 22,
              fontWeight: FontWeight.w700,
              letterSpacing: 4,
              color: Cores.secundario,
            ),
          ),
          SizedBox(height: 6),
          Text(
            'Criado neste aparelho, a partir da sua chave.',
            style: TextStyle(
              fontSize: 11,
              height: 1.6,
              color: Cores.textoCorrido,
            ),
          ),
          SizedBox(height: 10),
          _Botao(rotulo: 'GERAR CÓDIGO', aoTocar: null),
        ],
      ),
    );
  }
}

class _CasasPin extends StatelessWidget {
  const _CasasPin({required this.preenchidas});

  final int preenchidas;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      label: 'PIN: $preenchidas de $tamanhoPin dígitos',
      excludeSemantics: true,
      child: Row(
        children: [
          for (var i = 0; i < tamanhoPin; i++) ...[
            if (i > 0) const SizedBox(width: 8),
            Expanded(
              child: Container(
                key: Key('casa-$i'),
                height: 48,
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: Cores.painel,
                  border: Border.all(
                    color: i == preenchidas ? Cores.ambar : Cores.linha,
                    width: i == preenchidas ? 2 : 1,
                  ),
                ),
                child: i < preenchidas
                    ? Container(
                        key: Key('ponto-$i'),
                        width: 12,
                        height: 12,
                        color: Cores.ambar,
                      )
                    : null,
              ),
            ),
          ],
        ],
      ),
    );
  }
}

class _Teclado extends StatelessWidget {
  const _Teclado({required this.aoDigitar, required this.aoApagar});

  final void Function(int) aoDigitar;
  final VoidCallback aoApagar;

  @override
  Widget build(BuildContext context) {
    Widget tecla(String rotulo, VoidCallback? aoTocar, {String? semantica}) {
      return Expanded(
        child: Semantics(
          button: true,
          label: semantica ?? rotulo,
          excludeSemantics: true,
          child: InkWell(
            key: Key('tecla-$rotulo'),
            onTap: aoTocar,
            child: Container(
              // 48 dp: o mínimo de alvo de toque do Android (testado).
              height: 48,
              alignment: Alignment.center,
              decoration: BoxDecoration(border: Border.all(color: Cores.linha)),
              child: Text(
                rotulo,
                style: TextStyle(
                  fontSize: rotulo.length > 1 ? 11 : 20,
                  fontWeight: FontWeight.w600,
                  letterSpacing: rotulo.length > 1 ? 1 : 0,
                  color: rotulo.length > 1 ? Cores.secundario : Cores.texto,
                ),
              ),
            ),
          ),
        ),
      );
    }

    Widget linha(List<Widget> teclas) => Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Row(
        children: [
          for (var i = 0; i < teclas.length; i++) ...[
            if (i > 0) const SizedBox(width: 6),
            teclas[i],
          ],
        ],
      ),
    );

    Widget digito(int d) =>
        tecla('$d', () => aoDigitar(d), semantica: 'dígito $d');

    return Column(
      children: [
        linha([digito(1), digito(2), digito(3)]),
        linha([digito(4), digito(5), digito(6)]),
        linha([digito(7), digito(8), digito(9)]),
        linha([
          const Expanded(child: SizedBox(height: 48)),
          digito(0),
          tecla('APAGAR', aoApagar, semantica: 'apagar último dígito'),
        ]),
      ],
    );
  }
}

class _Aviso extends StatelessWidget {
  const _Aviso();

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
            'Não há número, e-mail nem conta em servidor. A chave e o PIN ficam só neste aparelho.',
            style: TextStyle(
              fontSize: 11,
              height: 1.6,
              color: Cores.textoCorrido,
            ),
          ),
        ],
      ),
    );
  }
}

class _Botao extends StatelessWidget {
  const _Botao({required this.rotulo, required this.aoTocar});

  final String rotulo;
  final VoidCallback? aoTocar;

  @override
  Widget build(BuildContext context) {
    final ligado = aoTocar != null;
    return Semantics(
      button: true,
      enabled: ligado,
      label: rotulo,
      excludeSemantics: true,
      child: InkWell(
        key: Key('botao-$rotulo'),
        onTap: aoTocar,
        child: Container(
          height: 48,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: ligado ? Cores.ambar : Colors.transparent,
            border: Border.all(color: ligado ? Cores.ambar : Cores.linha),
          ),
          child: Text(
            rotulo,
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w700,
              letterSpacing: 2,
              color: ligado ? Cores.fundo : Cores.secundario,
            ),
          ),
        ),
      ),
    );
  }
}

class _Rodape extends StatelessWidget {
  const _Rodape({required this.versaoNucleo});

  final String versaoNucleo;

  @override
  Widget build(BuildContext context) {
    return Container(
      key: const Key('rodape'),
      padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 10),
      decoration: const BoxDecoration(
        border: Border(top: BorderSide(color: Cores.linha)),
      ),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Text(
            'NÚCLEO v$versaoNucleo',
            key: const Key('versao-nucleo'),
            style: estiloRotulo,
          ),
          const Text('BUILD DE TESTE', style: estiloRotulo),
        ],
      ),
    );
  }
}
