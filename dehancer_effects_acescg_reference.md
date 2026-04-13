# Dehancer — effets, paramètres, réalité physique, constantes de départ (workflow ACEScg)

> **But du document**
> Fournir une base de travail exploitable dans VS Code / Codex pour recréer un pipeline **physiquement crédible** inspiré de Dehancer.
>
> **Important**
> - Les **noms des effets** et leurs **paramètres** ci-dessous viennent de la documentation publique Dehancer.
> - Les **constantes physiques** proposées ici sont des **valeurs de départ d'ingénierie** pour un prototype, **pas** des mesures officielles publiées par Dehancer.
> - Nous partons du principe que le moteur travaille en **ACEScg** pour tous les calculs internes.

---

## 1) Hypothèse pipeline globale

### Espace de travail
- **Working space** : **ACEScg**
- **Nature** : espace **linéaire**, primaires **AP1**
- **Usage** : très adapté aux opérations de compositing, diffusion lumineuse, blur, bloom, halation, mélange additif, réponse optique

### Pourquoi ACEScg ici
- Les effets lumineux sont plus cohérents en **linéaire** qu'en sRGB/Rec.709 gamma-encodé.
- ACEScg couvre un **large gamut**, utile pour préserver les couleurs intenses avant la sortie display.
- Attention : certaines conversions vers AP1 peuvent générer des **valeurs négatives hors gamut**, donc il faut prévoir soit un clamp prudent, soit une **Reference Gamut Compression** avant l'output si nécessaire.

### Règle pratique
- **Importer** → convertir vers **ACEScg linéaire**
- **Calculer tous les effets** en ACEScg
- **Afficher / exporter** via transform de sortie vers l'espace cible

---

## 2) Rappels ACEScg utiles au moteur

### Constantes ACEScg de base
- **Primaires AP1**
  - Red: x=0.713, y=0.293
  - Green: x=0.165, y=0.830
  - Blue: x=0.128, y=0.044
- **White point** : ACES white point normalisé de l'ACES system
- **Encoding** : linéaire (gamma 1.0)

### Règles d'implémentation
- Faire les **blur**, **convolutions**, **additions lumineuses** et **mélanges optiques** en **linéaire**.
- Éviter de calculer halation/bloom après tone mapping final.
- Pour les effets type film/print, garder une séparation claire entre :
  - **scène / optique / diffusion**
  - **émulsion / développement / print**
  - **display rendering**

---

## 3) Classification physique des effets

### A. Pré-correction / préparation du signal
- Input / Source corrections
- Expand
- LUT Generator
- Monitor / False Color
- Output / Total Impact

### B. Phénomènes pellicule / chimie / tirage
- Film Profiles
- Push / Pull
- Film Developer
- Film Compression
- Print
- CMY Color Head & Print Toning
- Film Grain
- Halation

### C. Phénomènes optiques
- Bloom
- Vignette
- Defringe (correction d'aberrations)

### D. Phénomènes mécaniques / support / vieillissement
- Film Breath
- Gate Weave
- Film Damage
- Overscan

---

## 4) Effets Dehancer — fiche complète

---

## 4.1 Input / Source Corrections

### Paramètres Dehancer
- Exposure Comp.
- Temperature Comp.
- Tint Comp.
- Defringe

### Réalité physique derrière
- **Exposure Comp.** : compensation d'exposition de la source avant émulation.
- **Temperature / Tint** : correction de balance colorimétrique en amont.
- **Defringe** : réduction des **aberrations chromatiques** de l'optique/capteur qui peuvent interférer avec Bloom/Halation.

### Nature
- Pas un effet argentique autonome.
- C'est une **préparation du signal**.

### Constantes / valeurs de départ proposées
```txt
exposure_comp_ev      = [-2.0 ; +2.0]
temperature_shift     = [-1.0 ; +1.0]
tint_shift            = [-1.0 ; +1.0]
defringe_amount       = [0.0 ; 1.0]
```

---

## 4.2 Film Profiles

### Paramètres Dehancer
- Film Profile
- Push / Pull (Ev)

### Réalité physique derrière
- Simulation de la **courbe de réponse** d'une pellicule donnée.
- Le **Push/Pull** représente une sous- ou sur-exposition suivie d'un développement adapté.
- Effets attendus :
  - contraste
  - latitude
  - roll-off des hautes lumières
  - séparation tonale
  - rendu couleur

### Nature
- **Pellicule + développement labo**

### Constantes / valeurs de départ proposées
```txt
push_pull_ev          = [-3.0 ; +3.0]
base_contrast_gamma   = 0.90 .. 1.25
shoulder_strength     = 0.10 .. 0.60
toe_strength          = 0.05 .. 0.40
color_density_bias    = 0.80 .. 1.20
```

---

## 4.3 Film Developer

### Paramètres Dehancer
- Contrast Boost
- Gamma Correction
- Color Separation
- Color Boost

### Réalité physique derrière
- Variation liée au **révélateur** et au **process chimique de développement**.
- Influence la pente de la courbe, la séparation colorée et la densité visuelle.

### Nature
- **Chimie du développement**

### Constantes / valeurs de départ proposées
```txt
contrast_boost        = [0.0 ; 2.0]
gamma_correction      = [0.70 ; 1.30]
color_separation      = [0.0 ; 2.0]
color_boost           = [0.0 ; 2.0]
```

---

## 4.4 Film Compression

### Paramètres Dehancer
- Impact
- White Point
- Tonal Range
- Color Density

### Réalité physique derrière
- Compression progressive des hautes lumières proche du comportement d'un **négatif**.
- Permet un **shoulder** plus doux qu'un clipping numérique brutal.

### Nature
- **Réponse sensitométrique film-like**

### Constantes / valeurs de départ proposées
```txt
compression_impact    = [0.0 ; 1.0]
white_point_norm      = [0.50 ; 1.20]
tonal_range_norm      = [0.0 ; 1.0]
color_density         = [0.0 ; 2.0]
```

### Modèle conseillé
- Courbe type shoulder paramétrique
- Compression plus forte au-dessus d'un seuil scene-linear
- Préserver la chroma de façon contrôlée

---

## 4.5 Expand

### Paramètres Dehancer
- Black Point
- White Point
- Color Mode / Luma mode

### Réalité physique derrière
- Ajustement manuel de la plage tonale en sortie de profil film.
- Pas un phénomène physique autonome : c'est un **outil de calage**.

### Nature
- Outil technique

### Constantes / valeurs de départ proposées
```txt
black_point           = [0.0 ; 0.2]
white_point           = [0.8 ; 1.2]
luma_only_mode        = true / false
```

---

## 4.6 Print

### Paramètres Dehancer
- Print Medium
  - Linear
  - Cineon Film Log
  - Kodak 2383
  - Fujifilm 3513
  - Kodak Endura Glossy Paper
- Target White
- Exposure (Ev)
- Tonal Contrast
- Color Density
- Saturation
- Analogue Range Limiter

### Réalité physique derrière
- Simulation du **tirage optique** vers une copie positive de projection ou un papier photo.
- Le print influe fortement sur :
  - contraste final
  - densité couleur
  - teinte globale
  - réponse des hautes lumières et basses lumières

### Nature
- **Étape de tirage analogique**

### Constantes / valeurs de départ proposées
```txt
print_exposure_ev     = [-2.0 ; +2.0]
print_contrast        = [0.5 ; 2.0]
print_color_density   = [0.5 ; 2.0]
print_saturation      = [0.0 ; 2.0]
range_limiter         = [0.0 ; 1.0]
```

### Print mediums classiques à proposer dans un MVP
- **Kodak 2383** : print film cinéma très classique
- **Fujifilm 3513** : alternative print cinéma
- **Kodak Endura Glossy** : papier photo couleur classique
- **Linear** : utile comme bypass neutre de comparaison

---

## 4.7 CMY Color Head & Print Toning

### Paramètres Dehancer
- CMY Color Head
- Toning Shadows / Midtones / Highlights
- Preserve Exposure
- Impact

### Réalité physique derrière
- Simule la correction par **tête couleur subtractive CMY** des agrandisseurs / systèmes de tirage.
- En analogique, changer les filtres CMY modifie aussi l'exposition du tirage.

### Nature
- **Tirage / printer lights / filtration subtractive**

### Constantes / valeurs de départ proposées
```txt
cyan_head             = [-1.0 ; +1.0]
magenta_head          = [-1.0 ; +1.0]
yellow_head           = [-1.0 ; +1.0]
shadow_toning         = [-1.0 ; +1.0]
midtone_toning        = [-1.0 ; +1.0]
highlight_toning      = [-1.0 ; +1.0]
preserve_exposure     = true / false
impact                = [0.0 ; 1.0]
```

---

## 4.8 Film Grain

### Paramètres Dehancer
- Grain Profile
- Film Type: Negative / Positive
- Processing Mode: Analogue / Noise
- Size
- Amount
- Shadows
- Midtones
- Highlights
- Film Resolution
- Chroma

### Réalité physique derrière
- Le grain n'est pas un overlay ; il est lié à la **granularité de l'émulsion**, à la densité et à la structure des couches colorées.
- La visibilité du grain varie selon la zone tonale.

### Nature
- **Émulsion**

### Constantes / valeurs de départ proposées
```txt
grain_size_px_ref     = 0.35 .. 2.50
grain_amount          = [0.0 ; 1.0]
grain_shadows         = [0.0 ; 2.0]
grain_midtones        = [0.0 ; 2.0]
grain_highlights      = [0.0 ; 2.0]
film_resolution       = [0.5 ; 1.5]
grain_chroma          = [0.0 ; 1.0]
```

### Modèle conseillé
- Grain dépendant de la luminance locale
- Taille liée au format simulé
- Répartition différente pour négatif vs positif
- Variante rapide possible : bruit corrélé multi-échelle

### Formats classiques à proposer
- 8 mm
- 16 mm
- 35 mm
- 65 mm

---

## 4.9 Halation

### Paramètres Dehancer
- Halation Profile
- Source Limiter
- Background Gain
- Smoothness
- Local Diffusion
- Global Diffusion
- Amplify
- Hue
- Blue Comp.
- Impact
- Mask Mode

### Réalité physique derrière
- Halo rouge/orangé autour des très fortes sources, reflets spéculaires et bords contrastés.
- Causé par la diffusion/réémission de lumière dans la structure film, fortement influencée par la présence ou non de couche anti-halation.
- Les versions **No Remjet** correspondent à une suppression de la couche anti-halation, donnant un effet plus fort.

### Nature
- **Phénomène pellicule / émulsion**

### Constantes / valeurs de départ proposées
```txt
halation_threshold_ev         = [-4.0 ; +4.0]        # seuil relatif en stops
halation_source_limiter       = [0.0 ; 1.0]
halation_background_gain      = [0.0 ; 1.0]
halation_smoothness           = [0.0 ; 1.0]
halation_local_radius_px      = 1.0 .. 20.0
halation_global_radius_px     = 8.0 .. 120.0
halation_amplify              = [0.0 ; 4.0]
halation_hue_shift            = [-0.25 ; +0.25]
halation_blue_comp            = [0.0 ; 1.0]
halation_opacity              = [0.0 ; 1.0]
```

### Constantes physiques proposées pour un modèle crédible
```txt
halation_red_bias             = 1.00
halation_green_bias           = 0.28 .. 0.55
halation_blue_bias            = 0.00 .. 0.12
halation_energy_preserve      = 0.85 .. 1.00
halation_anisotropy           = 0.0  # MVP = isotrope
```

### Pipeline conseillé
1. Extraire les hautes lumières / bords contrastés
2. Pondérer par la luminance du fond
3. Diffusion locale
4. Diffusion globale
5. Coloration rouge-orange
6. Compensation fond froid via `blue_comp`
7. Composite additif contrôlé

### Pellicules classiques à proposer pour l'inspiration du profil
- **Kodak Vision3 50D 5203 / 7203**
- **Kodak Vision3 250D 5207 / 7207**
- **Kodak Vision3 500T 5219 / 7219**
- Variante spéciale : **No Remjet / CineStill-like** pour un halation exagéré

---

## 4.10 Bloom

### Paramètres Dehancer
- Bloom Profile
- Highlights
- Source Limiter
- Details
- Diffusion
- Amplify
- Save Lights
- Saturation
- Impact
- Mask Mode

### Réalité physique derrière
- Diffusion lumineuse due d'abord à l'**optique**, puis amplifiée visuellement dans l'image finale.
- Contrairement à un simple soft focus, le bloom apparaît surtout **autour des sources lumineuses**.

### Nature
- **Optique + amplification visuelle image**

### Constantes / valeurs de départ proposées
```txt
bloom_highlights             = [0.0 ; 1.0]
bloom_source_limiter         = [0.0 ; 1.0]
bloom_details                = [0.0 ; 1.0]
bloom_diffusion_radius_px    = 4.0 .. 200.0
bloom_amplify                = [0.0 ; 4.0]
bloom_save_lights            = [0.0 ; 1.0]
bloom_saturation             = [0.0 ; 1.0]
bloom_opacity                = [0.0 ; 1.0]
```

### Constantes physiques proposées
```txt
veiling_glare_floor          = 0.0 .. 0.05
lens_scatter_strength        = 0.0 .. 1.0
source_core_protection       = 0.0 .. 1.0
```

### Pipeline conseillé
1. Détecter les sources brillantes
2. Construire masque multi-échelle
3. Diffusion large
4. Préserver le cœur des lumières avec `save_lights`
5. Conserver ou réduire la saturation héritée de la source
6. Composite additif doux

### Optiques / situations à simuler
- Optique moderne multicouche → bloom plus contenu
- Optique vintage / coatings faibles → bloom plus diffus
- Source dans ou proche du cadre → plus de veiling glare

---

## 4.11 Film Breath

### Paramètres Dehancer
- Film Breath Profile
- Period
- Exposure
- Tonal Contrast
- Color
- Impact

### Réalité physique derrière
- Micro-variations d'exposition, contraste et couleur d'une image à l'autre dues aux irrégularités de process, mécanique et chimie.

### Nature
- **Instabilité temporelle de support / process**

### Constantes / valeurs de départ proposées
```txt
breath_period_frames         = 8 .. 120
breath_exposure_ev           = 0.00 .. 0.30
breath_contrast              = 0.00 .. 0.20
breath_color_shift           = 0.00 .. 0.10
breath_impact                = [0.0 ; 1.0]
```

---

## 4.12 Gate Weave

### Paramètres Dehancer
- Gate Weave Profile
- Period
- Translation X
- Translation Y
- Rotation
- Auto Zoom
- Impact

### Réalité physique derrière
- Micro-dérive mécanique de la pellicule dans la gate, scanner ou projecteur.

### Nature
- **Mécanique**

### Constantes / valeurs de départ proposées
```txt
weave_period_frames          = 4 .. 80
weave_tx_px                  = 0.0 .. 3.0
weave_ty_px                  = 0.0 .. 3.0
weave_rotation_deg           = 0.0 .. 0.25
auto_zoom                    = true / false
weave_impact                 = [0.0 ; 1.0]
```

---

## 4.13 Film Damage

### Paramètres Dehancer
#### Dust
- Amount
- Scale
- Size Balance
- White-Black
- Enabled

#### Hairs
- Amount
- Scale
- Size Balance
- White-Black
- Enabled

#### Scratches
- Amount
- Scale
- Size Balance
- White-Black
- Enabled

#### Global Settings
- Total Amount
- Global Period
- Global Opacity
- Global Chromaticity

### Réalité physique derrière
- Poussières, cheveux, rayures, irrégularités d'émulsion, salissures de projection/scan.
- Leur apparence dépend de la transparence, épaisseur, profondeur, distance à la surface et interaction avec la lumière.

### Nature
- **Support / vieillissement / manutention**

### Constantes / valeurs de départ proposées
```txt
dust_amount                  = [0.0 ; 1.0]
dust_scale                   = [0.0 ; 2.0]
dust_size_balance            = [0.0 ; 1.0]
dust_white_black             = [-1.0 ; +1.0]

hairs_amount                 = [0.0 ; 1.0]
hairs_scale                  = [0.0 ; 2.0]
hairs_size_balance           = [0.0 ; 1.0]
hairs_white_black            = [-1.0 ; +1.0]

scratches_amount             = [0.0 ; 1.0]
scratches_scale              = [0.0 ; 2.0]
scratches_size_balance       = [0.0 ; 1.0]
scratches_white_black        = [-1.0 ; +1.0]

global_amount                = [0.0 ; 1.0]
global_period_frames         = 1 .. 240
global_opacity               = [0.0 ; 1.0]
global_chromaticity          = [0.0 ; 1.0]
```

---

## 4.14 Vignette

### Paramètres Dehancer
- Exposure
- Size
- Feather
- Aspect Ratio
- Center X / Y

### Réalité physique derrière
- Chute de lumière liée à l'optique, au design du fût, à l'angle d'incidence et parfois à des choix créatifs.

### Nature
- **Optique**

### Constantes / valeurs de départ proposées
```txt
vignette_exposure_ev         = [-4.0 ; +4.0]
vignette_size                = [0.0 ; 1.0]
vignette_feather             = [0.0 ; 1.0]
vignette_aspect_ratio        = [0.5 ; 2.0]
vignette_center_x            = [-1.0 ; +1.0]
vignette_center_y            = [-1.0 ; +1.0]
```

### Modèle conseillé
- Masque radial/elliptique paramétrique
- Option physique simple : loi douce proche cos^4 adaptée visuellement

---

## 4.15 Overscan

### Paramètres Dehancer
- Effet listé par Dehancer, mais paramètres exacts non détaillés ici dans les sources ouvertes consultées.

### Réalité physique derrière
- Bord de frame film, marge de gate, perforations, scan hors zone image utile.

### Nature
- **Caméra / scan / projection**

### Note d'honnêteté
- À compléter si tu veux ce module plus tard, mais ce n'est pas prioritaire pour un MVP halation/bloom.

---

## 4.16 Monitor / False Color

### Paramètres Dehancer
- Activation False Color

### Réalité physique derrière
- Aucune : outil de monitoring d'exposition.

### Nature
- Outil technique

---

## 4.17 LUT Generator

### Paramètres Dehancer
- LUT Size: 17x17x17 / 33x33x33
- Disable Input Transform

### Réalité physique derrière
- Aucune : outil d'export du look.

### Nature
- Outil technique

### Note importante
- Les LUT ne peuvent pas représenter fidèlement les effets spatiaux/locaux comme grain, halation, bloom, gate weave.

---

## 4.18 Output / Total Impact

### Paramètres Dehancer
- Total Impact

### Réalité physique derrière
- Aucune : master amount global.

### Nature
- Outil technique

---

## 5) Pellicules classiques connues à proposer dans ton moteur

> Ici, le but n'est pas de copier exactement Dehancer, mais de proposer des familles de rendu crédibles.

### Négatifs couleur cinéma
- **Kodak Vision3 50D 5203 / 7203**
  - grain fin
  - contraste modéré
  - daylight
  - halation contenu
- **Kodak Vision3 250D 5207 / 7207**
  - plus polyvalent
  - rendu cinéma très classique
- **Kodak Vision3 500T 5219 / 7219**
  - tungsten
  - plus de texture/grain perçu
  - très bonne base pour tests halation nocturne

### Positifs / print / projection
- **Kodak 2383 Print Film**
- **Fujifilm 3513 Print Film**

### Photo couleur négative classique
- **Kodak Portra 160**
- **Kodak Portra 400**
- **Kodak Gold 200**
- **Kodak Ektar 100**
- **Fujifilm Pro 400H** (historique / référence esthétique)
- **Kodak Endura Glossy Paper** pour la logique de print photo

### Variantes “look halation fort”
- **CineStill-like / No Remjet style**
  - base utile pour tests de halation plus marqué
  - à traiter comme profil spécial, pas comme vérité universelle

---

## 6) Priorité MVP si le but est “être proche du réel”

### Ordre conseillé
1. **ACEScg ingest + output transform propre**
2. **Bloom**
3. **Halation**
4. **Film Compression**
5. **Film Grain**
6. **Print**
7. Vignette
8. Tools annexes

### Pourquoi
- Bloom + Halation donnent tout de suite la signature lumineuse.
- Film Compression donne un roll-off crédible.
- Grain ajoute la texture.
- Print ancre le rendu dans une vraie logique argentique.

---

## 7) Constantes physiques MVP recommandées

### Pack Bloom par défaut
```txt
bloom_threshold_luma         = 4.0
bloom_soft_knee              = 1.2
bloom_radius_px              = 48.0
bloom_amplify                = 0.35
bloom_saturation             = 0.85
bloom_save_lights            = 0.70
veiling_glare_floor          = 0.01
```

### Pack Halation par défaut
```txt
halation_threshold_luma      = 6.0
halation_local_radius_px     = 6.0
halation_global_radius_px    = 28.0
halation_amplify             = 0.65
halation_opacity             = 0.45
halation_red_bias            = 1.00
halation_green_bias          = 0.38
halation_blue_bias           = 0.05
halation_background_gain     = 0.70
halation_blue_comp           = 0.20
```

### Pack Film Compression par défaut
```txt
compression_impact           = 0.35
compression_white_point      = 1.00
compression_tonal_range      = 0.45
compression_color_density    = 0.85
```

### Pack Grain 35 mm neutre
```txt
grain_size_px_ref            = 0.65
grain_amount                 = 0.22
grain_shadows                = 1.15
grain_midtones               = 1.00
grain_highlights             = 0.80
grain_chroma                 = 0.18
film_resolution              = 1.00
```

---

## 8) Ce qui est “physique” vs “artistique”

### Principalement physiques
- Film Profiles
- Film Developer
- Film Compression
- Print
- Film Grain
- Halation
- Bloom
- Vignette
- Gate Weave
- Film Breath
- Film Damage

### Principalement outils de contrôle
- Input corrections
- Expand
- LUT Generator
- Monitor
- Output / Total Impact

---

## 9) Notes d'implémentation pour Codex

### Règles simples
- Travailler en **float16/float32** en interne
- Tous les effets lumineux en **ACEScg linéaire**
- Ne pas faire de halation après tone mapping final
- Séparer les passes :
  - source mask
  - diffusion
  - coloration
  - composite

### Modules Rust/WGPU conseillés
```txt
src/
  color/
    acescg.rs
    transforms.rs
  effects/
    bloom.rs
    halation.rs
    film_compression.rs
    grain.rs
    print.rs
  shaders/
    bloom_extract.wgsl
    bloom_blur_h.wgsl
    bloom_blur_v.wgsl
    halation_extract.wgsl
    halation_blur_h.wgsl
    halation_blur_v.wgsl
    halation_composite.wgsl
    film_compression.wgsl
```

---

## 10) Sources publiques utilisées

### Dehancer
- Dehancer Learn
- Dehancer Photo Plugin Quick Guide (2024)
- Articles Dehancer sur : Halation, Bloom, Film Profiles, Film Developer, Film Compression, CMY Color Head & Print Toning, Input

### ACES
- Documentation ACES officielle
- ACEScg encoding docs
- ACES working spaces
- Reference Gamut Compression docs

---

## 11) Honnêteté méthodologique

Ce document mélange trois niveaux :
1. **les paramètres officiels publics Dehancer**
2. **la lecture physique plausible** de ces paramètres
3. **des constantes de départ proposées** pour un moteur personnel Rust/WebGPU en ACEScg

Les constantes proposées sont donc des **valeurs d'ingénierie pour prototype**, pas des “secrets Dehancer”.

