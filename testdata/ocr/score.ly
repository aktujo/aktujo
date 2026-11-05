% SPDX-License-Identifier: AGPL-3.0-or-later
% A made-up part of sheet music, notes written for the OCR spike. Its words are in score.txt.
\version "2.24.0"
\header {
  title = "Abendlied am Fluss"
  subtitle = "für Blasorchester"
  composer = "Musik: Anna Feldmann"
  arranger = "Arr.: Peter Lindqvist"
  instrument = "Trompete 2 in B"
  tagline = ##f
}
\paper { #(set-paper-size "a4") }
\relative c'' {
  \clef treble \key f \major \time 4/4 \tempo "Andante" 4 = 76
  f4 g a f | a8 bes c4 c2 | d4 c bes a | g2. r4 |
  f4 g a f | a8 bes c4 c2 | d4 c bes g | f1 \mark \default
  a4. bes8 c4 a | g4. a8 bes4 g | f4 e d e | f2 r2 |
  c'4\mf bes a g | a4 bes c2 | d4 c bes a | g1 \bar "|."
}
