// Peças comuns das telas de entrada (primeira abertura, desbloqueio, cofre aberto).
// Visual aprovado em 06/11/2026 (linha "KRYPTOS unificado"); cores em tema.dart.

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

const estiloTextoCorrido = TextStyle(
  fontSize: 11,
  height: 1.6,
  color: Cores.textoCorrido,
);

const estiloAviso = TextStyle(fontSize: 11, height: 1.6, color: Cores.laranja);

/// Esqueleto das telas de entrada: cabeçalho, conteúdo que rola, botão de ação
/// sempre inteiro fora da rolagem e rodapé com a versão do núcleo.
class MoldeDeEntrada extends StatelessWidget {
  const MoldeDeEntrada({
    super.key,
    required this.titulo,
    required this.versaoNucleo,
    required this.conteudo,
    this.acao,
  });

  final String titulo;
  final String versaoNucleo;
  final List<Widget> conteudo;
  final Widget? acao;

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
              Cabecalho(titulo: titulo),
              Expanded(
                child: SingleChildScrollView(
                  padding: const EdgeInsets.fromLTRB(18, 14, 18, 12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: conteudo,
                  ),
                ),
              ),
              if (acao != null)
                Padding(
                  padding: const EdgeInsets.fromLTRB(18, 10, 18, 10),
                  child: acao,
                ),
              Rodape(versaoNucleo: versaoNucleo),
            ],
          ),
        ),
      ),
    );
  }
}

class Cabecalho extends StatelessWidget {
  const Cabecalho({super.key, required this.titulo});

  final String titulo;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 14),
      decoration: const BoxDecoration(
        border: Border(bottom: BorderSide(color: Cores.ambar, width: 2)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // Linha de base comum, como no visual aprovado. O rótulo da direita é
          // Expanded: com fonte grande do sistema ele quebra a linha em vez de estourar.
          Row(
            crossAxisAlignment: CrossAxisAlignment.baseline,
            textBaseline: TextBaseline.alphabetic,
            children: [
              const Text(
                'KRYPTOS',
                style: TextStyle(
                  fontSize: 22,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 6,
                  color: Cores.ambar,
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Text(
                  titulo,
                  textAlign: TextAlign.right,
                  style: estiloRotulo,
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),
          const Row(
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

/// Painel com o código de contato (ou os pontos, enquanto não há identidade).
class PainelCodigo extends StatelessWidget {
  const PainelCodigo({
    super.key,
    required this.codigo,
    required this.legenda,
    this.rodape = const [],
  });

  final String? codigo;
  final String legenda;
  final List<Widget> rodape;

  @override
  Widget build(BuildContext context) {
    final gerado = codigo != null;
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: Cores.painel,
        border: Border.all(color: Cores.linha),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const Text('CÓDIGO DE CONTATO', style: estiloRotulo),
          const SizedBox(height: 6),
          Text(
            codigo ?? '····-····',
            key: const Key('codigo'),
            style: TextStyle(
              fontSize: 22,
              fontWeight: FontWeight.w700,
              letterSpacing: 4,
              color: gerado ? Cores.texto : Cores.secundario,
            ),
          ),
          const SizedBox(height: 6),
          Text(legenda, style: estiloTextoCorrido),
          ...rodape,
        ],
      ),
    );
  }
}

class CasasPin extends StatelessWidget {
  const CasasPin({super.key, required this.preenchidas});

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

class Teclado extends StatelessWidget {
  const Teclado({
    super.key,
    required this.aoDigitar,
    required this.aoApagar,
    this.ligado = true,
  });

  final void Function(int) aoDigitar;
  final VoidCallback aoApagar;

  /// Desligado enquanto o núcleo confere ou grava: toque não faz nada.
  final bool ligado;

  @override
  Widget build(BuildContext context) {
    Widget tecla(String rotulo, VoidCallback aoTocar, {String? semantica}) {
      return Expanded(
        child: Semantics(
          button: true,
          enabled: ligado,
          label: semantica ?? rotulo,
          excludeSemantics: true,
          child: InkWell(
            key: Key('tecla-$rotulo'),
            onTap: ligado ? aoTocar : null,
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

/// Botão do visual: âmbar quando ligado, só contorno quando desligado.
/// [nome] fixa a chave do widget (`botao-<nome>`) mesmo quando o rótulo muda
/// (por exemplo CONTINUAR → GRAVANDO…).
class Botao extends StatelessWidget {
  const Botao({
    super.key,
    required this.rotulo,
    required this.aoTocar,
    this.nome,
  });

  final String rotulo;
  final String? nome;
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
        key: Key('botao-${nome ?? rotulo}'),
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

/// Aviso em laranja (alerta), com chave para os testes. Ocupa o lugar do texto de
/// apoio embaixo das casas do PIN, para a tela não crescer.
class Aviso extends StatelessWidget {
  const Aviso(this.texto, {super.key});

  final String texto;

  @override
  Widget build(BuildContext context) =>
      Text(texto, key: const Key('aviso'), style: estiloAviso);
}

class Rodape extends StatelessWidget {
  const Rodape({super.key, required this.versaoNucleo});

  final String versaoNucleo;

  @override
  Widget build(BuildContext context) {
    return Container(
      key: const Key('rodape'),
      padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 10),
      decoration: const BoxDecoration(
        border: Border(top: BorderSide(color: Cores.linha)),
      ),
      // Os dois lados quebram a linha em vez de estourar: com fonte de 160% num
      // celular de 360 dp, os dois rótulos juntos passam da largura.
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: Text(
              'NÚCLEO v$versaoNucleo',
              key: const Key('versao-nucleo'),
              style: estiloRotulo,
            ),
          ),
          const SizedBox(width: 12),
          const Flexible(
            child: Text(
              'BUILD DE TESTE',
              textAlign: TextAlign.right,
              style: estiloRotulo,
            ),
          ),
        ],
      ),
    );
  }
}
