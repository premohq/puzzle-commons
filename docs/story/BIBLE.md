# Puzzle Commons: story bible

Tone: a cute, funny anime road trip on the surface (bickering, snacks, puns, puzzle nerdery),
with a quiet sad mystery underneath that surfaces in a few hard-hitting moments. All ages: no
gore, no cruelty for its own sake, but real loss, guilt and loneliness. Think "the comedy makes
the sad parts land". Every world ends warmer than it began. Original writing only: no references
to real games, franchises, brands, or real people.

## The world
Five worlds, each the home of one puzzle style, used to meet once a year at **the Commons**: one
huge shared board where everyone played together, every style at once.

**Surface story (what Mira tells the player):** Queen Kaede of Cinder Crown split the styles
into five separate worlds and closed the Commons. Mira, a puzzle courier, travels with her friend
Kai to beat each world's champion, borrow their style, and reopen the Commons.

**The hidden truth (revealed gradually; full reveal in world 5):**
- Seven years ago, at the last Commons Cup final, the shared board **overflowed** (the "Great
  Top-Out"). In this world, when a board tops out, a little of whoever was playing stays behind
  as gray garbage blocks. The Commons board was so big that when it overflowed, it swallowed
  people. One of them was **Sena**, Mira's older sister, who was playing the final against Kaede.
- Kaede sent the last attack. She has believed for seven years that she buried Sena. She split
  the worlds so no board could ever grow that big again. Her "tyranny" is grief and guilt.
- Mira's **puzzle box** holds the last surviving piece of the Commons board. Mira has told
  everyone she is "delivering it". The truth: she has been hearing Sena's voice from it at night.
  She is not reopening the Commons for fun. She wants her sister back.
- **Kai** was there. He was the ball boy at the final. He saw Sena push Mira (then little) off
  the board as it overflowed, and Sena asked him to look after Mira. He has spent seven years
  quietly trying to keep Mira away from Cinder Crown (losing on purpose, "forgetting" maps,
  terrible snacks to slow her down). He thinks reopening the Commons might take Mira too.
- Gray garbage blocks everywhere are "what's left over". This is never said outright until
  world 3 (Yuki's clinic) and world 5. Before that it is only hinted (Hana's garden grows on old
  gray blocks; Nami's lighthouse shines for "ships" that never come back).

## Ending (epilogue)
The Commons reopen. Every style plays on one board. When the gray blocks at the bottom of the
Commons finally clear, Sena does not come back as a person. Her voice speaks once, warmly: she
was never trapped; she stayed to keep the board from swallowing anyone else, and now that
everyone plays together, the board is light enough that she can rest. She tells Mira to "keep
delivering". Bittersweet, hopeful. Kaede and Mira forgive each other. Kai finally eats a decent
snack. Last line is funny and warm.

## Cast (speaker ids, voice, comedy, hidden layer)
- **mira**: puzzle courier, 17, pink hair, cat-ear headband. Upbeat, competitive, signs off
  deliveries ("Signed, sealed, stacked!"). Treats everything as a delivery. Hidden: Sena; the
  voice in the box; she is less okay than she pretends.
- **kai**: her friend, 18, hoodie, deadpan, self-deprecating. Running gag: brings horrible snacks
  (pickled marshmallows, "chain-flavored" gum). Pretends to be bad at puzzles; secretly brilliant
  at Chain. Hidden: Sena's promise; he has been sabotaging the trip out of love and fear.
- **nami**: Quad Reef lighthouse keeper. Prim, perfectionist, makes nautical and line-clear puns
  and is embarrassed by them. Thinks blocks should only ever fall straight down. Hidden: her
  parents were sailors who went to the last Commons Cup and never came back; she keeps the light
  on for them every night.
- **hana**: Bloom Valley gardener. Dreamy, talks to the blobs and names every one of them (Gerald,
  Sproutlet, Mrs. Puddle...). Slow, patient, chains. Hidden: her garden grows on a field of old gray
  garbage blocks; she plants flowers on them like gravestones and knows exactly what they are.
- **yuki**: Clinic Moon night doctor. Deadpan, clinical, gallows humor ("Prognosis: you lose.").
  Hidden: her "patients" are people who partly topped out at the Commons and never fully woke;
  she has kept them asleep and safe for seven years. She is the one who tells Mira what gray
  blocks really are (dark moment, handled gently).
- **rin**: Tile Bazaar merchant. Loud, greedy-seeming, haggles everything, comic relief.
  Hidden: she is broke because she secretly gives everything away to families of the lost; her
  "prices" go to them. She sold Kaede the idea of splitting the worlds and regrets it.
- **kaede**: Queen of Cinder Crown. Regal, icy, dramatic, secretly awkward and very bad at
  Chain. Hidden: the guilt above. She practises against an empty seat every night.
- **sora, ren, taro, emi**: arcade regulars who cameo for comedy (Sora brags about records, Ren
  is calm until row eleven, Taro plays on the night train, Emi chains first and asks questions
  later). They are NOT in on the mystery.
- **Sena** never appears on screen (no model). Her lines can be quoted by others, or appear as
  speaker id `mira` reading them aloud? NO: if Sena speaks, write the line as spoken by `mira`
  with expression `shocked` and text in quotes, e.g. `"Keep delivering, Mira." (The box. It's her voice.)`
  Use this very sparingly (the epilogue).

## Running gags (use them, don't overuse)
- Kai's snacks.
- Mira's courier sign-offs.
- Nami's puns and instant regret.
- Hana naming blobs.
- Yuki's medical deadpan.
- Rin trying to charge for everything, including dialogue.
- Everyone mispronouncing "Commons" as something else once.

## Structure and scene ids (each world: 4 scenes, plus prologue and epilogue)
- `prologue`: world 0 backdrop. Mira and Kai set off. Cute, funny. One tiny unsettling beat (the
  box hums at night; Kai notices, says nothing).
- `wN_intro`: arriving in world N; meet the champion. Mostly comedy, light hint of their secret.
- `wN_mid`: plays before the 3rd node of world N (new scene). A quieter conversation, usually
  Mira and Kai, or Mira and the champion. This is where depth leaks through.
- `wN_boss`: before the champion's match (node 6). Stakes and banter.
- `wN_clear`: after beating the champion. The champion lends their style; one honest moment.
- `epilogue`: after Kaede. The full payoff described above. Backdrop world 0.

World order: 0 Quad Reef (nami), 1 Bloom Valley (hana), 2 Clinic Moon (yuki), 3 Tile Bazaar (rin),
4 Cinder Crown (kaede). Escalation: worlds 0-1 light with hints; world 2 first dark reveal (gray
blocks are people's leftovers; Yuki's patients); world 3 Kai's secret comes out (Rin knows Kai
from seven years ago; argument; Kai confesses Sena's promise; Mira is hurt); world 4 Kaede's guilt,
the fight, the truth about the final, the reconciliation.

## Rules for every line
- Speaker ids: mira, kai, nami, hana, yuki, rin, kaede, sora, ren, taro, emi.
- Expressions (only these): normal, smile, delighted, angry, sad, shocked, smug, sleepy.
- Max 140 characters per line (it must fit three lines of the dialogue box). Prefer 60-110.
- 5 to 12 lines per scene. Snappy. No narration lines; everything is spoken.
- `RIGHT` lists who stands on the right side (usually the world's champion, or kai when he and
  mira are alone). Everyone else stands on the left.
- No em dashes; use commas, colons or full stops. Plain ASCII quotes and apostrophes.

## Story-map node lines
Each world has 6 nodes; each has a name and one line Mira says on the map card that teaches a
mechanic. Keep the mechanic tip (it is a tutorial) but add personality or a story nudge. Max 130
characters. Node 6 is the champion's match. Current mechanic tips per node are given to you; keep
the same mechanic.

## Output format (exactly; it is parsed by a script)
```
SCENE w0_intro
RIGHT nami
mira smile | Line text here.
nami smug | Line text here.
END

NODE 0 0 Lantern Shore | Line text here.
```
One file per writer. No other text outside SCENE/END blocks and NODE lines except lines starting
with `#` (comments, ignored).

## Inspirations from puzzle-game lore (themes only: never names, characters, places or plots)
The user wants the story to feel like it belongs to the great puzzle-game traditions. Borrow the
*kinds* of storytelling those games do, in our own words and world:
- Worlds colliding: separate puzzle "dimensions" bleeding into each other is both the disaster and
  the cure (the Commons is literally every style on one board).
- The lonely antagonist: a powerful figure who causes chaos because they are lonely or grieving, and
  is a bit ridiculous about it (Kaede's dramatic speeches, secretly practising against an empty seat).
- Friends under a spell: champions acting cold or strange because of something that happened to
  them, "freed" by a heartfelt match rather than defeated.
- A shared catastrophe that makes rival worlds unite (the Great Top-Out; the finale needs everyone).
- Puzzle play as feeling: rhythm, music, color and the calm "flow" of clearing lines as emotional
  language (Mira talks about the sound boards make; the Commons used to sing).
- Deadpan medicine against invading germs (Clinic Moon's tone).
- Cute mascots and slapstick between serious beats: the comedy and the sincerity share a scene.
Keep everything original: invent new terms, places and jokes. No references to real games, ever.

## Revision: someone dies and comes back (the finale)
The user wants a death and a return. It is **Kai**, and it mirrors Sena:
- `w4_clear` (after beating Kaede): forgiveness, Kaede agrees, the Commons reopen. Everyone arrives.
- NEW `w4_fall` (plays right after `w4_clear`): the first match on the reopened Commons starts joyful, then the
  board overflows again (seven years of gray rows were never cleared). Mira slips toward the top line; Kai
  pushes her clear, exactly as Sena did, and is swallowed. His last line is a joke about his snacks. He is
  gone: a new gray row with a hoodie-blue streak. Silence. Kaede breaks: "Not again." This must land hard.
- `epilogue` becomes the return: nobody leaves. Every champion takes a side of the Commons and they play
  together, all styles, all night, clearing gray rows one by one (Nami's light guiding, Hana naming the rows,
  Yuki keeping count, Rin giving everything away, Kaede finally chaining). When the hoodie-blue row clears,
  Sena's voice speaks once through the box (speaker `mira`, expression `shocked`, quoted, short parenthetical):
  she held the board up for seven years and has one last push in her; she sends Kai back and can finally
  rest. Kai comes back, blinking, alive, hungry. Bittersweet joy. Last line funny and warm.
