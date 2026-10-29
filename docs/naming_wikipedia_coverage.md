# Wikipedia (ja) “IUPAC命名法”: naming coverage

Every example of the Japanese Wikipedia article [IUPAC命名法](https://ja.wikipedia.org/wiki/IUPAC%E5%91%BD%E5%90%8D%E6%B3%95) (oldid 111380645, fetched 2026-10-29), including the names written in its figures, with the name the namer (`src/naming`) gives and the test that checks it. Tracking issue: OPE-20 (children OPE-21 to OPE-25; OPE-26, Japanese names, was canceled).

Policy: one preferred IUPAC name (PIN) per structure, the other names separately (`iupac_alternatives`, OPE-24). Where Wikipedia and the IUPAC 2013 recommendations (the Red Book 2005 for inorganic names) differ, the IUPAC name is expected and the reason is in a comment beside the test case.

Status: **pass**: the test asserts Wikipedia’s name. **IUPAC-2013 override**: the test asserts the IUPAC name, which differs from Wikipedia’s (reason in the note). **out of scope**: deliberately not produced. **gap**: not named; the namer refuses the structure with a reason (never a wrong name). **rule, no structure**: a general rule without a concrete compound. **extension**: not a Wikipedia example; a case added beside one (the groups Wikipedia’s seniority list leaves out), with the namer’s result in the note.

Tests are `src/naming/<file>.rs::<fn>`; `words::tests` is the test module of `src/naming/words.rs`.

## Summary

| status | count |
|---|---|
| pass | 162 |
| IUPAC-2013 override | 22 |
| out of scope | 2 |
| gap | 7 |
| rule, no structure | 3 |
| **total (Wikipedia’s English names)** | **196** |
| extension (not in Wikipedia) | 3 |
| **total rows** | **199** |
| Japanese names: out of scope (OPE-26 canceled) | 151 |

Gaps:

- Replacement nomenclature (figure, DBU), bicycloundecene (figure: 6- and 7-membered rings fused, C=C in the 7-ring): two ortho-fused rings of five or more members take fusion names (P-52.2.4.1); fusion nomenclature is not implemented, so the structure is refused with that reason.
- Replacement nomenclature (figure, DBU), diazabicycloundecene (figure): fusion name, as above. The figure puts the C=N in the seven-membered ring: an isomer of DBU, whose amidine is in the six-membered ring.
- Replacement nomenclature (figure, DBU), DBU (text): fusion names are not implemented; real DBU (C1CCC2=NCCCN2CC1) is refused with the reason. Wikipedia’s fusion name fits the drawn isomer, not DBU.
- General rules: seniority of groups (P-41), acid + anhydride: refused: an anhydride as a substituent (acyloxycarbonyl) is not built.
- General rules: seniority of groups (P-41), anhydride + ester: refused: “anhydride together with an ester” (the ester would be an alkoxycarbonyl prefix of one acid, which is not built).
- General rules: seniority of groups (P-41), amine + imine: imines are not supported (refused: “nitrogen group other than amine…”, also with an amine present: 4-iminopentan-1-amine).
- Penicillin G walk-through, Step 1: azete and thiazole fused: fusion nomenclature is not implemented. With a four-membered ring the von Baeyer name is the PIN (P-52.2.4.1; Wikipedia also takes it), so only this alternative is missing.

## Examples

| section | example (structure or formula) | expected name | kind | test | status | note |
|---|---|---|---|---|---|---|
| Inorganic: elements | O3 (open O–O–O) | trioxygen | PIN | `inorganic_tests::elements` | pass |  |
| Inorganic: elements | O3 (open O–O–O) | ozone | alternative (retained) | `alternatives_tests::retained_names` | pass | Red Book IR-3.4.3; not for cyclo-O3 (`alternatives_tests::rings_are_not_chains`) |
| Inorganic: salts | NaCl | sodium chloride | PIN | `inorganic_tests::salts_of_separate_ions` | pass |  |
| Inorganic: salts | KNaSO4 | potassium sodium sulfate | PIN | `inorganic_tests::several_cations` | pass |  |
| Inorganic: salts | NaHCO3 | sodium hydrogencarbonate | PIN | `inorganic_tests::oxoanions` | IUPAC override (Red Book 2005) | Wikipedia: “sodium hydrogen carbonate”, the older spelling; IR-8.4 and IR-8.5 write one word |
| Policy: retained names not derivatised | –CH(CH3)–CH2Cl | 1-chloropropan-2-yl | PIN (prefix) | `tests::wikipedia_acyclic_examples` | IUPAC-2013 override | Wikipedia: “1-chloro-2-propyl”; the 2013 form cites the free-valence locant before “yl” (P-29.2) |
| Policy: retained names not derivatised | –CH(CH3)–CH2Cl | never “1-chloroisopropyl” | alternative (absent) | `alternatives_tests::retained_names_are_not_derivatised` | pass |  |
| Substitutive nomenclature | ClCH2–CHCl2 | 1,1,2-trichloroethane | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Functional class nomenclature | CH3OH | methyl alcohol | alternative (functional class) | `alternatives_tests::radicofunctional_names` | pass |  |
| Additive nomenclature | C4H8O (ring) | oxolane | PIN | `tests::heterocycles` | pass | Hantzsch–Widman name |
| Additive nomenclature | C4H8O (ring) | tetrahydrofuran | alternative (additive) | `alternatives_tests::additive_names` | pass |  |
| Subtractive nomenclature | ribose − O | deoxyribose | alternative (subtractive) | `alternatives_tests::no_subtractive_or_japanese_names` | out of scope | subtractive names of sugars (P-31.1.5, P-102) are out of scope (OPE-24); the structure gets its oxolane/oxane name |
| Conjunctive nomenclature (figure) | cyclohexane (figure) | cyclohexane | PIN | `tests::table_of_names` | pass |  |
| Conjunctive nomenclature (figure) | CH3OH (figure) | methanol | PIN | `tests::table_of_names` | pass |  |
| Conjunctive nomenclature (figure) | C6H11–CH2OH | cyclohexanemethanol | alternative (conjunctive) | `alternatives_tests::conjunctive_names` | pass |  |
| Conjunctive nomenclature (figure) | C6H11–CH2OH | cyclohexylmethanol | PIN | `alternatives_tests::conjunctive_names` | pass | P-15.6.0: conjunctive names are not used in PINs |
| Replacement nomenclature (figure, DBU) | bicycloundecene (figure: 6- and 7-membered rings fused, C=C in the 7-ring) | (a fusion name) | PIN | `tests::wikipedia_ring_examples` | gap | two ortho-fused rings of five or more members take fusion names (P-52.2.4.1); fusion nomenclature is not implemented, so the structure is refused with that reason |
| Replacement nomenclature (figure, DBU) | diazabicycloundecene (figure) | (a fusion name; the von Baeyer replacement name as an alternative) | PIN | `tests::wikipedia_ring_examples` | gap | fusion name, as above. The figure puts the C=N in the seven-membered ring: an isomer of DBU, whose amidine is in the six-membered ring |
| Replacement nomenclature (figure, DBU) | DBU (text) | 2,3,4,5,8,9,10,11-octahydropyrido[2,1-b][1,3]diazepine (Wikipedia) | alternative | `tests::wikipedia_ring_examples` | gap | fusion names are not implemented; real DBU (C1CCC2=NCCCN2CC1) is refused with the reason. Wikipedia’s fusion name fits the drawn isomer, not DBU |
| General rules: seniority of groups (P-41) | Wikipedia’s list as a whole | carboxy > anhydride > ester > amide > nitrile > aldehyde > ketone > alcohol > amine > imine > (ether, nitro, halo) | rule | `tests::wikipedia_seniority_of_groups` | rule, no structure | checked pair by pair below |
| General rules: seniority of groups (P-41) | acid + anhydride | — | rule pair | `tests::wikipedia_seniority_of_groups` | gap | refused: an anhydride as a substituent (acyloxycarbonyl) is not built |
| General rules: seniority of groups (P-41) | acid + nitrile: HOOC–CH2CH2–CN | 3-cyanopropanoic acid | PIN | `tests::wikipedia_seniority_of_groups` | pass |  |
| General rules: seniority of groups (P-41) | anhydride + ester | — | rule pair | `tests::wikipedia_seniority_of_groups` | gap | refused: “anhydride together with an ester” (alkoxycarbonyl prefix not built) |
| General rules: seniority of groups (P-41) | anhydride + nitrile | acetic 3-cyanopropanoic anhydride | PIN | `tests::wikipedia_seniority_of_groups` | pass |  |
| General rules: seniority of groups (P-41) | ester + amide | methyl 4-carbamoylbenzoate | PIN | `tests::wikipedia_seniority_of_groups` | pass |  |
| General rules: seniority of groups (P-41) | amide + nitrile | 2-cyanoacetamide; 4-cyanobenzamide | PIN | `tests::wikipedia_seniority_of_groups` | pass |  |
| General rules: seniority of groups (P-41) | nitrile + aldehyde | 3-oxopropanenitrile | PIN | `tests::wikipedia_seniority_of_groups` | pass |  |
| General rules: seniority of groups (P-41) | aldehyde + ketone | 3-oxobutanal | PIN | `tests::wikipedia_seniority_of_groups` | pass |  |
| General rules: seniority of groups (P-41) | ketone + alcohol | 1-hydroxypropan-2-one | PIN | `tests::wikipedia_seniority_of_groups` | pass |  |
| General rules: seniority of groups (P-41) | alcohol + amine | 2-aminoethan-1-ol | PIN | `tests::wikipedia_seniority_of_groups` | pass |  |
| General rules: seniority of groups (P-41) | amine + imine | — | rule pair | `tests::wikipedia_seniority_of_groups` | gap | imines are not supported (refused: “nitrogen group other than amine…”; tested with CC(=N)C and with H2N–(CH2)3–C(=NH)–CH3) |
| General rules: seniority of groups (P-41) | ether, nitro, halo: prefixes only | 1-methoxy-2-nitroethane; 1-chloro-2-nitroethane; 1-chloro-2-methoxyethane | PIN | `tests::wikipedia_seniority_of_groups` | pass |  |
| General rules: seniority of groups (P-41) | acid halides (missing from Wikipedia’s list) | acid halide > amide: 4-carbamoylbenzoyl chloride | PIN | `tests::wikipedia_seniority_of_groups` | extension | named (pass); P-41 ranks acid halides between esters and amides |
| General rules: seniority of groups (P-41) | ester + acid halide (not in Wikipedia’s list) | — | rule pair | `tests::wikipedia_seniority_of_groups` | extension | not named (refused): the acid-halide prefix (carbonochloridoyl or chloro…oxo) is not decided |
| General rules: seniority of groups (P-41) | thiols (missing from Wikipedia’s list) | alcohol > thiol > amine: 2-sulfanylpentan-1-ol; 2-aminoethane-1-thiol | PIN | `tests::wikipedia_seniority_of_groups` | extension | named (pass); P-41 class 17: OH > SH, both above amines |
| General rules: principal chain | CH3–CH2–CH2–CH(C2H5)–CH3 | 3-methylhexane (not 2-ethylpentane) | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| General rules: principal chain | R1(R2)C=C(R3)R4: R3–C–R4 must not be the chain | CH2=C(C2H5)(C3H7): 3-methylidenehexane | rule | `tests::substituent_edge_cases` | IUPAC-2013 override | Wikipedia gives the 1979/1993 rule (unsaturation first); IUPAC 2013 takes the longest chain first (P-44.3), so the chain may pass through the sp2 carbon |
| General rules: locants | lowest locants from the nearer end, then the principal group | — | rule | (throughout, e.g. tests::wikipedia_acyclic_examples) | rule, no structure |  |
| Substituent groups | C5H11– (unbranched) | pentyl | PIN (prefix) | `tests::wikipedia_acyclic_examples` | pass |  |
| Substituent groups | –CH2–CH2–CH2– | propane-1,3-diyl | PIN (prefix) | `tests::wikipedia_acyclic_examples` | IUPAC-2013 override | Wikipedia: “propylene”; P-29.3.2.2 gives propane-1,3-diyl. “propylene” is given only as a traditional name of –CH(CH3)–CH2– (`alternatives_tests::traditional_names`) |
| Substituent groups | CH3(CH2)4CO– | hexanoyl | PIN (prefix) | `tests::wikipedia_acyclic_examples` | pass |  |
| Substituent groups | C5H11O– (unbranched) | pentyloxy | PIN (prefix) | `tests::wikipedia_acyclic_examples` | pass |  |
| Substituent groups | CH3O– | methoxy | PIN (prefix) | `tests::substituent_groups` | pass |  |
| Substituent groups | C2H5O– | ethoxy | PIN (prefix) | `tests::substituent_groups` | pass |  |
| Substituent groups | C3H7O– | propoxy | PIN (prefix) | `tests::substituent_groups` | pass |  |
| Substituent groups | C4H9O– | butoxy | PIN (prefix) | `tests::substituent_groups` | pass |  |
| Substituent groups | CnH2n+1– with n = 0 | hydrogen atom | rule | — | rule, no structure |  |
| Numerical terms | 1 | mono- (alone); hen- (combined) | numerical term | `words::tests::numerical_terms` | pass | “mono” is not written in substitutive names (`multiplier(1)` is empty); “hen” is checked in 21, 101, 1001 |
| Numerical terms | 2 | di- (alone); do- (combined) | numerical term | `words::tests::numerical_terms` | pass | “do” is checked in 22, 52, 102 |
| Numerical terms | 3 | tri- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 4 | tetra- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 5 | penta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 6 | hexa- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 7 | hepta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 8 | octa- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 9 | nona- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 10 | deca- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 11 | undeca- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 12 | dodeca- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 20 | icosa- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 21 | henicosa- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 22 | docosa- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 30 | triaconta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 40 | tetraconta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 50 | pentaconta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 60 | hexaconta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 70 | heptaconta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 80 | octaconta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 90 | nonaconta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 100 | hecta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 200 | dicta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 300 | tricta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 400 | tetracta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 500 | pentacta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 600 | hexacta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 700 | heptacta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 800 | octacta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 900 | nonacta- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 1000 | kilia- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 2000 | dilia- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 3000 | trilia- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 4000 | tetralia- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 5000 | pentalia- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 6000 | hexalia- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 7000 | heptalia- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 8000 | octalia- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 9000 | nonalia- | numerical term | `words::tests::numerical_terms` | pass |  |
| Numerical terms | 2567 | heptahexacontapentactadilia- | numerical term | `words::tests::numerical_terms`; `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C1 | methane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C2 | ethane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C3 | propane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C4 | butane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C5 | pentane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C6 | hexane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C7 | heptane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C8 | octane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C9 | nonane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C10 | decane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C11 | undecane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C12 | dodecane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C13 | tridecane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C14 | tetradecane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C15 | pentadecane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C16 | hexadecane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C17 | heptadecane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C18 | octadecane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C19 | nonadecane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C20 | icosane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C21 | henicosane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C22 | docosane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C23 | tricosane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C24 | tetracosane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C25 | pentacosane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C26 | hexacosane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C27 | heptacosane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C28 | octacosane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C29 | nonacosane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C30 | triacontane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C40 | tetracontane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C50 | pentacontane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C60 | hexacontane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C70 | heptacontane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C80 | octacontane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C90 | nonacontane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C100 | hectane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C200 | dictane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C300 | trictane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C400 | tetractane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C500 | pentactane | PIN | `tests::alkane_names_up_to_9999` | IUPAC-2013 override | Wikipedia: “pentacane”; IUPAC 2013 Table 1.4: pentacta (500) |
| Alkanes (unbranched) | C600 | hexactane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C700 | heptactane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C800 | octactane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C900 | nonactane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C1000 | kiliane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C2000 | diliane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C3000 | triliane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C4000 | tetraliane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C5000 | pentaliane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C6000 | hexaliane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C7000 | heptaliane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C8000 | octaliane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkanes (unbranched) | C9000 | nonaliane | PIN | `tests::alkane_names_up_to_9999` | pass |  |
| Alkenes, alkynes | CH3CH2CH=CHCH3 | pent-2-ene | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Alkenes, alkynes | cis-/trans- before the name | cis-pent-2-ene, trans-pent-2-ene | alternative | `stereo_tests::cis_trans_alternatives` | pass | the PIN uses Z/E: (2Z)-/(2E)-pent-2-ene |
| Alkenes, alkynes | CH3(CH2)3C≡CCH2CH3 | oct-3-yne | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Alkenes, alkynes | CH3CH2CH=CHCH=CHCH3 | hepta-2,4-diene | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Halogenoalkanes | CH3(CH2)3Cl | 1-chlorobutane | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Halogenoalkanes | CH3(CH2)3Cl | butyl chloride | alternative (functional class) | `alternatives_tests::radicofunctional_names` | pass |  |
| Halogenoalkanes | CBr3–CBr3 | hexabromoethane | PIN | `tests::wikipedia_acyclic_examples` | IUPAC-2013 override | Wikipedia: “perbromoethane”; “per” is no longer recommended (P-14.3.4.5) |
| Halogenoalkanes | CBr3–CBr3 | perbromoethane | alternative (traditional) | `alternatives_tests::traditional_names` | pass |  |
| Alcohols | HOCH2–CH(OH)–CH2OH | propane-1,2,3-triol | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Alcohols | HOCH2–CH(OH)–CH2OH | glycerol | alternative (retained) | `alternatives_tests::retained_names` | pass |  |
| Alcohols | CH3–CH(OH)–CHO | 2-hydroxypropanal | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Alcohols | CH3–(CH2)2–OH | propyl alcohol | alternative (functional class) | `alternatives_tests::radicofunctional_names` | pass | PIN propan-1-ol |
| Ethers | CH3–O–C2H5 | methoxyethane | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Ethers | CH3–O–C2H5 | ethyl methyl ether | alternative (functional class) | `alternatives_tests::radicofunctional_names` | pass | Wikipedia gives it only in Japanese (エチルメチルエーテル) |
| Aldehydes, ketones | HCHO | formaldehyde | PIN | `tests::wikipedia_acyclic_examples` | IUPAC-2013 override | Wikipedia: “methanal”, with formaldehyde as the common name; formaldehyde is the retained PIN (P-66.6.1.1.1) |
| Aldehydes, ketones | HCHO | methanal | alternative (systematic) | `alternatives_tests::systematic_names` | pass |  |
| Aldehydes, ketones | C2H5–CO–CH3 | butan-2-one | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Carboxylic acids | HOOC–(CH2)2–COOH | butanedioic acid | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Carboxylic acids | HOOC–(CH2)2–COOH | succinic acid | alternative (retained) | `alternatives_tests::retained_names` | pass |  |
| Carboxylic acids | C6H5–COOH | benzoic acid | PIN | `tests::wikipedia_acyclic_examples` | IUPAC-2013 override | Wikipedia: “benzenecarboxylic acid”; benzoic acid is the retained PIN (P-65.1.1.1) |
| Carboxylic acids | C6H5–COOH | benzenecarboxylic acid | alternative (systematic) | `alternatives_tests::systematic_names` | pass |  |
| Carboxylic acids | HOOC–CH2–CH(COOH)–CH2–COOH | propane-1,2,3-tricarboxylic acid | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Carboxylic acids | HOOC–(CH2)2–CH(CH2COOH)–(CH2)2–COOH | 4-(carboxymethyl)heptanedioic acid | PIN | `tests::wikipedia_acyclic_examples` | IUPAC-2013 override | Wikipedia: “2-(carboxymethyl) heptanedioic acid”; the branch is on C-4 from either end (and no space) |
| Carboxylate salts, esters | C2H5–COO⁻ K⁺ | potassium propanoate | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Carboxylate salts, esters | C2H5–COO⁻ K⁺ | potassium propionate | alternative (retained) | `alternatives_tests::retained_names` | pass | Wikipedia gives it only in Japanese (プロピオン酸カリウム) |
| Carboxylate salts, esters | C2H5–COO–C2H5 | ethyl propanoate | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Carboxylate salts, esters | C2H5–COO–C2H5 | ethyl propionate | alternative (retained) | `alternatives_tests::retained_names` | pass | Wikipedia gives it only in Japanese (プロピオン酸エチル) |
| Acyl halides, anhydrides | CH3–COCl | acetyl chloride | PIN | `tests::wikipedia_acyclic_examples` | IUPAC-2013 override | Wikipedia: “ethanoyl chloride”, with acetyl chloride as the common name; acetyl is the retained preferred prefix (P-65.5.1) |
| Acyl halides, anhydrides | CH3–COCl | ethanoyl chloride | alternative (systematic) | `alternatives_tests::systematic_names` | pass |  |
| Acyl halides, anhydrides | C2H5–CO–O–CO–C2H5 | propanoic anhydride | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Acyl halides, anhydrides | CH3–CO–O–CO–C2H5 | acetic propanoic anhydride | PIN | `tests::wikipedia_acyclic_examples` | IUPAC-2013 override | Wikipedia: “ethanoic propanoic anhydride”; acetic acid is the retained PIN (P-65.7.2) |
| Acyl halides, anhydrides | CH3–CO–O–CO–C2H5 | ethanoic propanoic anhydride | alternative (systematic) | `alternatives_tests::systematic_names` | pass |  |
| Amines, nitriles, thiols | CH3NH2 | methanamine | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Amines, nitriles, thiols | CH3NH2 | methylamine | alternative (functional class) | `alternatives_tests::radicofunctional_names` | IUPAC-2013 override | Wikipedia: “methyl amine”; one word (P-62.2.1.1.2) |
| Amines, nitriles, thiols | CH3NH2 | aminomethane | alternative (systematic) | `alternatives_tests::systematic_names` | pass |  |
| Amines, nitriles, thiols | C2H5CH(CN)–C2H5 | 2-ethylbutanenitrile | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Amines, nitriles, thiols | CH3–CH(SH)–(CH2)2–CH3 | pentane-2-thiol | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Amines, nitriles, thiols | CH2(OH)–CH(SH)–(CH2)2–CH3 | 2-sulfanylpentan-1-ol | PIN | `tests::wikipedia_acyclic_examples` | IUPAC-2013 override | Wikipedia: “2-sulfanyl pentan-1-ol”; no space |
| Rings | C4H8 (ring) | cyclobutane | PIN | `tests::wikipedia_acyclic_examples` | pass |  |
| Rings | cyclopentane and cyclohexane sharing one carbon | spiro[4.5]decane | PIN | `tests::wikipedia_ring_examples` | pass |  |
| Rings | (–CH=CH–)7 (ring) | cyclotetradeca-1,3,5,7,9,11,13-heptaene | PIN | `tests::wikipedia_ring_examples` | IUPAC-2013 override | Wikipedia: “[14]annulene”; the PIN is the cycloalkapolyene name (P-22.1.2) |
| Rings | (–CH=CH–)7 (ring) | [14]annulene | alternative (systematic) | `alternatives_tests::systematic_names` | pass |  |
| Rings | –O–CO–(CH2)5– (ring) | oxepan-2-one | PIN | `tests::wikipedia_ring_examples` | IUPAC-2013 override | Wikipedia: “oxepane-2-one”; the pseudoketone name (P-65.6.3.5.1), the “e” elided before “one” (P-16.7.1) |
| Rings | –O–CO–(CH2)5– (ring) | hexano-6-lactone | alternative (systematic) | `alternatives_tests::systematic_names` | IUPAC-2013 override | Wikipedia: “6-hexanelactone” (older rules); P-65.6.3.5.1 method (2) |
| Rings | –O–CO–(CH2)5– (ring) | ε-caprolactone | alternative (traditional) | `alternatives_tests::traditional_names` | pass |  |
| Rings | –NH–CO–(CH2)5– (ring) | azepan-2-one | PIN | `tests::wikipedia_ring_examples` | IUPAC-2013 override | Wikipedia: “azocan-2-one”, an eight-membered ring; the formula is a seven-membered ring (P-64.3.1) |
| Rings | –NH–CO–(CH2)5– (ring) | hexano-6-lactam | alternative (systematic) | `alternatives_tests::systematic_names` | IUPAC-2013 override | Wikipedia: “6-hexanelactam” (older rules); P-66.1.5.1 method (2) |
| Rings | –NH–CO–(CH2)5– (ring) | ε-caprolactam | alternative (traditional) | `alternatives_tests::traditional_names` | pass |  |
| Penicillin G walk-through | Step 1: “thienam” ring (trivial name) | — | alternative (trivial) | — | out of scope | Wikipedia itself rejects it (trivial, not recommended); trivial names of ring systems of natural products are not produced |
| Penicillin G walk-through | Step 1: azete and thiazole fused | 2,3,6,6a-tetrahydro-5H-azeto[2,3-b]thiazole (Wikipedia) | alternative | — | gap | fusion nomenclature is not implemented. With a four-membered ring the von Baeyer name is the PIN (P-52.2.4.1; Wikipedia also takes it), so only this alternative is missing |
| Penicillin G walk-through | Step 1 figure | bicyclo[3.2.0]heptane | PIN | `tests::wikipedia_ring_examples` | pass |  |
| Penicillin G walk-through | Step 2: thia and aza by replacement | 4-thia-1-azabicyclo[3.2.0]heptane | PIN | `tests::wikipedia_ring_examples` | pass | N lowest |
| Penicillin G walk-through | Step 2 figure (the lactam) | 4-thia-1-azabicyclo[3.2.0]heptan-7-one | PIN | `tests::wikipedia_ring_examples` | IUPAC-2013 override | figure: “…heptane-7-one”; the final “e” is elided before a vowel (P-16.7.1). Test case added in OPE-20 |
| Penicillin G walk-through | Step 3 figure (6-aminopenicillanic acid) | 6-amino-3,3-dimethyl-7-oxo-4-thia-1-azabicyclo[3.2.0]heptane-2-carboxylic acid | PIN | `tests::wikipedia_ring_examples` | IUPAC-2013 override | figure: “6-amino-2-carboxy-2,2-dimethyl-…heptane-7-one”: the acid is senior (P-41), and the methyl groups are on C-3, as the text says |
| Penicillin G walk-through | Step 4 figure | 3,3-dimethyl-7-oxo-6-(2-phenylacetamido)-4-thia-1-azabicyclo[3.2.0]heptane-2-carboxylic acid | PIN | `tests::wikipedia_ring_examples` | IUPAC-2013 override | figure: “6-phenylacetylamino-2-carboxy-2,2-dimethyl-…-7-one”; the amido prefix (P-66.1.1.4.3), alphanumerical order (OPE-22) |
| Penicillin G walk-through | Step 5: descriptors at C-2, C-5 and C-6 | 2S, 5R, 6R | PIN (stereo) | `stereo_tests::penicillin_g_full_name` | pass |  |
| Penicillin G walk-through | Step 5 figure and final name (structure as in Penicillin-G.svg) | (2S,5R,6R)-3,3-dimethyl-7-oxo-6-(2-phenylacetamido)-4-thia-1-azabicyclo[3.2.0]heptane-2-carboxylic acid | PIN | `stereo_tests::penicillin_g_full_name` | IUPAC-2013 override | Wikipedia: “(2S,5R,6R)-6-phenylacetylamino-2,2-dimethyl-7-oxo-…-2-carboxylic acid”: 3,3-dimethyl, the amido prefix, alphanumerical order (OPE-22) |

## Japanese names: out of scope (OPE-26 canceled)

The namer produces no Japanese names: `NameKind::Japanese` is reserved and never produced (`alternatives_tests::no_subtractive_or_japanese_names`). The English names of these examples are in the table above.

| where | Japanese name | status |
|---|---|---|
| examples | 三酸素 | out of scope (OPE-26 canceled) |
| examples | オゾン | out of scope (OPE-26 canceled) |
| examples | 塩化ナトリウム | out of scope (OPE-26 canceled) |
| examples | 硫酸カリウムナトリウム | out of scope (OPE-26 canceled) |
| examples | 炭酸水素ナトリウム | out of scope (OPE-26 canceled) |
| examples | ペンチル基 | out of scope (OPE-26 canceled) |
| examples | プロピレン基 | out of scope (OPE-26 canceled) |
| examples | ヘキサノイル基 | out of scope (OPE-26 canceled) |
| examples | ペンチロキシ基 | out of scope (OPE-26 canceled) |
| examples | メトキシ | out of scope (OPE-26 canceled) |
| examples | エトキシ | out of scope (OPE-26 canceled) |
| examples | プロポキシ | out of scope (OPE-26 canceled) |
| examples | ブトキシ | out of scope (OPE-26 canceled) |
| examples | 2-ペンテン | out of scope (OPE-26 canceled) |
| examples | 3-オクチン | out of scope (OPE-26 canceled) |
| examples | 2,4-ヘプタジエン | out of scope (OPE-26 canceled) |
| examples | 1-クロロブタン | out of scope (OPE-26 canceled) |
| examples | 塩化ブチル | out of scope (OPE-26 canceled) |
| examples | ペルブロモエタン | out of scope (OPE-26 canceled) |
| examples | グリセリン | out of scope (OPE-26 canceled) |
| examples | 1,2,3-プロパントリオール | out of scope (OPE-26 canceled) |
| examples | 2-ヒドロキシプロパナール | out of scope (OPE-26 canceled) |
| examples | プロピルアルコール | out of scope (OPE-26 canceled) |
| examples | エチルメチルエーテル | out of scope (OPE-26 canceled) |
| examples | メトキシエタン | out of scope (OPE-26 canceled) |
| examples | ホルムアルデヒド | out of scope (OPE-26 canceled) |
| examples | メタナール | out of scope (OPE-26 canceled) |
| examples | 2-ブタノン | out of scope (OPE-26 canceled) |
| examples | コハク酸 | out of scope (OPE-26 canceled) |
| examples | ブタン二酸 | out of scope (OPE-26 canceled) |
| examples | 安息香酸 | out of scope (OPE-26 canceled) |
| examples | ベンゼンカルボン酸 | out of scope (OPE-26 canceled) |
| examples | プロパン-1,2,3-トリカルボン酸 | out of scope (OPE-26 canceled) |
| examples | 2-（カルボキシメチル）ヘプタン二酸 | out of scope (OPE-26 canceled) |
| examples | プロピオン酸カリウム | out of scope (OPE-26 canceled) |
| examples | プロパン酸カリウム | out of scope (OPE-26 canceled) |
| examples | プロピオン酸エチル | out of scope (OPE-26 canceled) |
| examples | プロパン酸エチル | out of scope (OPE-26 canceled) |
| examples | 塩化アセチル | out of scope (OPE-26 canceled) |
| examples | 塩化エタノイル | out of scope (OPE-26 canceled) |
| examples | プロパン酸無水物 | out of scope (OPE-26 canceled) |
| examples | エタン酸プロパン酸無水物 | out of scope (OPE-26 canceled) |
| examples | メチルアミン | out of scope (OPE-26 canceled) |
| examples | メタンアミン | out of scope (OPE-26 canceled) |
| examples | アミノメタン | out of scope (OPE-26 canceled) |
| examples | 2-エチルブタンニトリル | out of scope (OPE-26 canceled) |
| examples | 2-ペンタンチオール | out of scope (OPE-26 canceled) |
| examples | 2-スルファニル-1-ペンタノール | out of scope (OPE-26 canceled) |
| examples | シクロブタン | out of scope (OPE-26 canceled) |
| examples | スピロ[4.5]デカン | out of scope (OPE-26 canceled) |
| examples | [14]アヌレン | out of scope (OPE-26 canceled) |
| examples | ε-カプロラクトン | out of scope (OPE-26 canceled) |
| examples | 6-ヘキサンラクトン | out of scope (OPE-26 canceled) |
| examples | オキセパン-2-オン | out of scope (OPE-26 canceled) |
| examples | ε-カプロラクタム | out of scope (OPE-26 canceled) |
| examples | 6-ヘキサンラクタム | out of scope (OPE-26 canceled) |
| examples | アゾカン-2-オン | out of scope (OPE-26 canceled) |
| numerical terms (1) | モノ | out of scope (OPE-26 canceled) |
| numerical terms (2) | ジ | out of scope (OPE-26 canceled) |
| numerical terms (3) | トリ | out of scope (OPE-26 canceled) |
| numerical terms (4) | テトラ | out of scope (OPE-26 canceled) |
| numerical terms (5) | ペンタ | out of scope (OPE-26 canceled) |
| numerical terms (6) | ヘキサ | out of scope (OPE-26 canceled) |
| numerical terms (7) | ヘプタ | out of scope (OPE-26 canceled) |
| numerical terms (8) | オクタ | out of scope (OPE-26 canceled) |
| numerical terms (9) | ノナ | out of scope (OPE-26 canceled) |
| numerical terms (10) | デカ | out of scope (OPE-26 canceled) |
| numerical terms (11) | ウンデカ | out of scope (OPE-26 canceled) |
| numerical terms (12) | ドデカ | out of scope (OPE-26 canceled) |
| numerical terms (20) | イコサ | out of scope (OPE-26 canceled) |
| numerical terms (21) | ヘンイコサ | out of scope (OPE-26 canceled) |
| numerical terms (22) | ドコサ | out of scope (OPE-26 canceled) |
| numerical terms (30) | トリアコンタ | out of scope (OPE-26 canceled) |
| numerical terms (40) | テトラコンタ | out of scope (OPE-26 canceled) |
| numerical terms (50) | ペンタコンタ | out of scope (OPE-26 canceled) |
| numerical terms (60) | ヘキサコンタ | out of scope (OPE-26 canceled) |
| numerical terms (70) | ヘプタコンタ | out of scope (OPE-26 canceled) |
| numerical terms (80) | オクタコンタ | out of scope (OPE-26 canceled) |
| numerical terms (90) | ノナコンタ | out of scope (OPE-26 canceled) |
| numerical terms (100) | ヘクタ | out of scope (OPE-26 canceled) |
| numerical terms (200) | デクタ | out of scope (OPE-26 canceled) |
| numerical terms (300) | トリクタ | out of scope (OPE-26 canceled) |
| numerical terms (400) | テトラクタ | out of scope (OPE-26 canceled) |
| numerical terms (500) | ペンタクタ | out of scope (OPE-26 canceled) |
| numerical terms (600) | ヘキサクタ | out of scope (OPE-26 canceled) |
| numerical terms (700) | ヘプタクタ | out of scope (OPE-26 canceled) |
| numerical terms (800) | オクタクタ | out of scope (OPE-26 canceled) |
| numerical terms (900) | ノナクタ | out of scope (OPE-26 canceled) |
| numerical terms (1000) | キラ | out of scope (OPE-26 canceled) |
| numerical terms (2000) | ジリア | out of scope (OPE-26 canceled) |
| numerical terms (3000) | トリリア | out of scope (OPE-26 canceled) |
| numerical terms (4000) | テトラリア | out of scope (OPE-26 canceled) |
| numerical terms (5000) | ペンタリア | out of scope (OPE-26 canceled) |
| numerical terms (6000) | ヘキサリア | out of scope (OPE-26 canceled) |
| numerical terms (7000) | ヘプタリア | out of scope (OPE-26 canceled) |
| numerical terms (8000) | オクタリア | out of scope (OPE-26 canceled) |
| numerical terms (9000) | ノナリア | out of scope (OPE-26 canceled) |
| alkanes (C1) | メタン | out of scope (OPE-26 canceled) |
| alkanes (C2) | エタン | out of scope (OPE-26 canceled) |
| alkanes (C3) | プロパン | out of scope (OPE-26 canceled) |
| alkanes (C4) | ブタン | out of scope (OPE-26 canceled) |
| alkanes (C5) | ペンタン | out of scope (OPE-26 canceled) |
| alkanes (C6) | ヘキサン | out of scope (OPE-26 canceled) |
| alkanes (C7) | ヘプタン | out of scope (OPE-26 canceled) |
| alkanes (C8) | オクタン | out of scope (OPE-26 canceled) |
| alkanes (C9) | ノナン | out of scope (OPE-26 canceled) |
| alkanes (C10) | デカン | out of scope (OPE-26 canceled) |
| alkanes (C11) | ウンデカン | out of scope (OPE-26 canceled) |
| alkanes (C12) | ドデカン | out of scope (OPE-26 canceled) |
| alkanes (C13) | トリデカン | out of scope (OPE-26 canceled) |
| alkanes (C14) | テトラデカン | out of scope (OPE-26 canceled) |
| alkanes (C15) | ペンタデカン | out of scope (OPE-26 canceled) |
| alkanes (C16) | ヘキサデカン | out of scope (OPE-26 canceled) |
| alkanes (C17) | ヘプタデカン | out of scope (OPE-26 canceled) |
| alkanes (C18) | オクタデカン | out of scope (OPE-26 canceled) |
| alkanes (C19) | ノナデカン | out of scope (OPE-26 canceled) |
| alkanes (C20) | イコサン | out of scope (OPE-26 canceled) |
| alkanes (C21) | ヘンイコサン | out of scope (OPE-26 canceled) |
| alkanes (C22) | ドコサン | out of scope (OPE-26 canceled) |
| alkanes (C23) | トリコサン | out of scope (OPE-26 canceled) |
| alkanes (C24) | テトラコサン | out of scope (OPE-26 canceled) |
| alkanes (C25) | ペンタコサン | out of scope (OPE-26 canceled) |
| alkanes (C26) | ヘキサコサン | out of scope (OPE-26 canceled) |
| alkanes (C27) | ヘプタコサン | out of scope (OPE-26 canceled) |
| alkanes (C28) | オクタコサン | out of scope (OPE-26 canceled) |
| alkanes (C29) | ノナコサン | out of scope (OPE-26 canceled) |
| alkanes (C30) | トリアコンタン | out of scope (OPE-26 canceled) |
| alkanes (C40) | テトラコンタン | out of scope (OPE-26 canceled) |
| alkanes (C50) | ペンタコンタン | out of scope (OPE-26 canceled) |
| alkanes (C60) | ヘキサコンタン | out of scope (OPE-26 canceled) |
| alkanes (C70) | ヘプタコンタン | out of scope (OPE-26 canceled) |
| alkanes (C80) | オクタコンタン | out of scope (OPE-26 canceled) |
| alkanes (C90) | ノナコンタン | out of scope (OPE-26 canceled) |
| alkanes (C100) | ヘクタン | out of scope (OPE-26 canceled) |
| alkanes (C200) | ジクタン | out of scope (OPE-26 canceled) |
| alkanes (C300) | トリクタン | out of scope (OPE-26 canceled) |
| alkanes (C400) | テトラクタン | out of scope (OPE-26 canceled) |
| alkanes (C500) | ペンタクタン | out of scope (OPE-26 canceled) |
| alkanes (C600) | ヘキサクタン | out of scope (OPE-26 canceled) |
| alkanes (C700) | ヘプタクタン | out of scope (OPE-26 canceled) |
| alkanes (C800) | オクタクタン | out of scope (OPE-26 canceled) |
| alkanes (C900) | ノナクタン | out of scope (OPE-26 canceled) |
| alkanes (C1000) | キリアン | out of scope (OPE-26 canceled) |
| alkanes (C2000) | ジリアン | out of scope (OPE-26 canceled) |
| alkanes (C3000) | トリリアン | out of scope (OPE-26 canceled) |
| alkanes (C4000) | テトラリアン | out of scope (OPE-26 canceled) |
| alkanes (C5000) | ペンタリアン | out of scope (OPE-26 canceled) |
| alkanes (C6000) | ヘキサリアン | out of scope (OPE-26 canceled) |
| alkanes (C7000) | ヘプタリアン | out of scope (OPE-26 canceled) |
| alkanes (C8000) | オクタリアン | out of scope (OPE-26 canceled) |
| alkanes (C9000) | ノナリアン | out of scope (OPE-26 canceled) |
