# Puzzle Commons: story bible

**Every spoiler is in this file.** The script itself is `crates/purtris-client/src/story.txt`
(plain text, parsed at startup). Check it with:

```
./target/release/puzzle-commons --story-check <dir> 1920 [scene ids to render...]
```

It reports parse errors, missing trigger scenes, any line that would overflow the dialogue box,
map-card lines that overflow, and node casts that don't match their seats. It also renders every
line of the scenes you name to `<dir>`.

## Tone

A cute, funny anime road trip on the surface (bickering, snacks, puns, puzzle nerdery), with grief
underneath that surfaces in a few hard moments. All ages: no gore, no cruelty for its own sake, but
real loss, guilt and loneliness. The comedy makes the sad parts land. Every world ends warmer than it
began. Original writing only: no references to real games, franchises, brands or people.

**The spine:** nobody should have to hold the board alone. Everyone in the story is carrying
something by themselves (a light, a field of stones, a ward, a secret, an empty chair, a box), and
every world ends with someone letting another person share the weight. The Commons died because
everyone attacked at once and nobody dug; it comes back when everyone digs together.

## The world

Five worlds, each the home of one puzzle style, used to meet once a year at **the Commons**: one huge
shared board where everyone played together, every style at once.

**What everyone knows:** seven years ago the Commons Cup final ended in an accident (the Great
Top-Out). Afterward Queen Kaede of Cinder Crown split the styles into five worlds and closed the
Commons. People assume she did it because she could.

**The truth, revealed in this order:**
1. **What gray garbage is** (hinted in worlds 1-2, said outright in world 3). When a board tops out,
   a little of whoever was playing stays behind as gray. Small boards leave a pebble you shake off by
   morning (that's why a loss feels heavy). The Commons left a field: it washed down the river into
   Bloom Valley, where Hana plants flowers on it.
2. **The sleepers** (world 3). Twenty-three players came off the big board breathing but not awake.
   Yuki has kept them in her quiet ward for seven years. Two of them are Nami's parents. One is Taro's
   brother Kenji.
3. **Kai's secret** (world 4). Kai was the ball boy at the final. Mira, aged ten, had snuck onto the
   board to sit with her sister Sena. As it overflowed, Sena pushed the last clean piece of the board
   into Mira's arms and threw her off the edge, to Kai. He caught her. Sena said "Look after her" and
   went under. That piece is Mira's box. Kai has spent seven years quietly steering Mira away from the
   Commons: forgotten maps, wrong ferries, losing by exactly one row, fake allergies.
4. **Why Kaede split the worlds** (world 5). Kaede was Sena's rival for six years and sent the last
   attack of the final. She believes she killed Sena, and split the worlds so no board could ever grow
   big enough to swallow anyone again. Every night she plays the empty, scorched seat from the final.
   Splitting the worlds was Rin's idea, given to Kaede for free.
5. **What really happened** (world 5). Toma, the archivist, has the record of the final, which Kaede
   never let him open. Its last line: "Seats sending: forty. Seats clearing: none." Everyone was
   attacking; nobody was digging. Kaede's chain was one of forty.
6. **What Mira carries** (world 5 finale). Sena never left the board. She has been holding it up from
   underneath for seven years, through the piece in Mira's box. That's what hums at night, off key,
   the way Sena hummed.

## The finale

- `w4_clear`: Kaede apologizes, Mira forgives her, and Kaede reopens the Commons on one condition:
  nobody plays alone on it. Every champion arrives. Nami read Mira's letter and visited her parents
  first.
- `w4_fall`: the first match on the reopened Commons is joyful, and everyone sends at once, on top of
  seven years of gray nobody cleared. It overflows again. Mira is pushed to the top line. Kai grabs
  her hand ("Holding on. Like you said."), then keeps the promise he made in world 4 and asks first:
  "Can I look after you?" She says yes without understanding, and he throws her clear to Kaede (as
  Sena threw her to him) and goes under. His last line is a joke about the marshmallow he gave the
  Queen. A new gray row with a streak of hoodie blue. Kaede: "Not again."
- `epilogue`: nobody goes home. Yuki's order: nobody sends, everybody digs. All night, every champion
  and every style clear gray rows together. Hana names the rows, Rin cracks her whole stall, Saki gets
  Kaede past her four-chain wall to five, and Toma counts the rows down. Mira remembers Mei's fortune
  ("lose someone at the top, find him at the bottom"). When the blue row clears, the box hums, and Sena
  speaks for the first and only time: she has one push left, sends Kai back, and can finally let go
  because everyone is holding the board now. She leaves Mira the sign-off to finish. Kai wakes up
  hungry ("You said you'd hold on!" / "You said yes first."). Clearing the old gray wakes all
  twenty-three sleepers, and the valley stones are gone, leaving only flowers. Nami goes home to put
  out the light with her parents watching. Kaede bakes, admits she ate the worst marshmallow, and Mira
  plans the next Commons Cup with Kai in the stands, in a helmet.

## Cast: voices, wants, flaws, arcs

Speaker ids are the cast ids in `cast.rs`. Two voices have no body: `box` (name tag "The box", only
hums) and `sena` (two lines in the epilogue, nowhere else).

- **mira**: courier, 17. Fast, bright, courier jargon (handbook, signature required, handle with
  care). Sign-off: "Signed, sealed, stacked!" *Wants* to reopen the Commons; *really* wants Sena back.
  *Flaw*: carries everything alone ("Nobody holds the box. I hold the box."). Hasn't told anyone the
  box hums. *Arc*: after the fight with Kai in world 4 she can't finish her sign-off; she finishes it
  when she forgives him, and finishes it one last time for Sena.
- **kai**: her oldest friend, 18, hoodie (always blue), deadpan, short sentences, understatement.
  Brings the cheapest local snack everywhere ("The snacks are just bad. That part was real.").
  Ball-boy reflexes. *Wants* to keep Mira safe. *Flaw*: decides for her and lies by omission.
  *Arc*: confesses, promises to ask first, plays for real (an eleven-chain in world 5; he practised in
  secret on the night train), then makes the one choice that is his to make.
- **nami** (Quad Reef): lighthouse keeper. Precise, counts things (212 stairs), corrects posture,
  nautical puns she instantly regrets. Keeps the light on for her parents, who took Haru's ferry to the
  final. Reason to fight: she won't leave the light, not for one night.
- **hana** (Bloom Valley): gardener. Slow, gentle, answers sideways, then says something devastatingly
  plain. Names every blob (Gerald, Sproutlet, Mrs. Puddle), and Mrs. Puddle's babies "Mira" (pink) and
  "Kai" (blue: "when someone's gone, people remember their color"). Tends the gray field and knows what
  it is. Reason to fight: she wants to know the courier will be gentle with the stones.
- **yuki** (Clinic Moon): night doctor. Deadpan diagnoses, invents diseases, never sleeps. Rule: never
  leave anyone at the bottom. Reason to fight: if the Commons tops out again, her ward fills.
- **rin** (Tile Bazaar): merchant, loud, charges for talking. Secretly gives her money to the families
  of the lost (Taro's mother, Aoi's wages, Nami's lamp oil). Recognizes Kai as the ball boy. Gave Kaede
  the idea of splitting the worlds.
- **kaede** (Cinder Crown): queen. Grand titles that collapse into awkwardness ("Royalty decorates.
  Stop reading my walls."). Her hands stop at a four-chain since the final. Keeps Kai's marshmallow in
  the royal treasury.

**Rivals with their own reasons** (one per story node; see `who` in `content.rs`):

| World | Node rivals |
|---|---|
| Quad Reef | Haru (ferryman: rowed Nami's parents out, won't row anyone toward the Commons without a match), Sora (defending his sprint record), Jun (sleepless ex-record-holder; would honestly like to lose and go to bed), Sora + Jun (teamed up because losing twice is "statistically insulting") vs Mira + Kai, Nami |
| Bloom Valley | Kiko (Hana's beekeeper friend, protects her from people who ask about the stones), Emi (chains first, plans never), Ren (collects gray stones for a shrine to the lost, one per person; loud at row eleven), Luna (moon courier, hired under a red crown seal to collect the box), Kiko + Emi vs Mira + Kai, Hana |
| Clinic Moon | Aoi (night nurse, counts sheep, counts everything), Taro (visits his sleeping brother on Thursdays and needs a good story to tell him), Luna (rematch; reveals the seal is the Queen's but the writer signed "T"), Aoi again, Aoi + Taro vs Mira + Kai, Yuki |
| Tile Bazaar | Mei (fortune teller: "always an S-piece", until today's fortune), Ren, Mira alone at the kiosk, Mei + Ren vs Mira + Luna (who turned down the job: "Handle as: a person"), Rin |
| Cinder Crown | Saki (Kaede's apprentice, fights to protect her), Toma (archivist, the "T" on the waybill), Saki + Toma vs Mira + Luna, Saki + Toma + Jun vs Mira + Kai + Luna, a free-for-all between Mira, Kai and Luna for breakfast, Kaede + Saki + Toma vs Mira + Kai + Luna |

Arcade regulars Sora, Jun, Emi and Ren are not in on the mystery, except that Ren's shrine is for the
lost.

## Setups and payoffs

| Setup | Payoff |
|---|---|
| Prologue: Mira hums the Commons tune wrong; the box hums the same wrong note; Kai: "Same wrong note." | World 5: Kaede says Sena hummed off key. Epilogue: Sena, "You hum it wrong too." |
| Prologue: Kai, to the box: "Nobody's going anywhere fast. I'll make sure of it." | Every forgotten map and one-row loss; confessed in world 4 |
| Kai loses by exactly one row (world 1), "sneezes" into a six-chain (world 2), "matchitis" (world 3), "losing on purpose? figure of speech" (world 3) | World 4 fight; world 5 eleven-chain |
| Nami's parents, blue sailcloth scarves | Found asleep holding hands in Yuki's ward; Mira's letter; Nami visits them; they wake in the epilogue |
| Haru: "The last two I rowed out said that." | Those two were Nami's parents |
| Hana's gray stones; "Please don't call them that"; flowers to look up at | Yuki's explanation of gray; the valley stones vanish in the epilogue |
| Hana names a blue blob Kai: people remember the color | The hoodie-blue row; "The blue one is Kai. We dig to Kai." |
| Taro's brother Kenji, bed by the window | Wakes with the other twenty-two |
| Luna's red seal with a crown, signed "T" | Kaede: "Toma. TOMA."; Toma wanted the box sealed because he believed the piece held someone |
| Mei: "You'll lose someone at the top, and find him at the bottom." | Kai goes under at the top line; Mira digs to the bottom row for him |
| Rin's "ball boy!" | Kai's confession |
| Kai's marshmallow for the Queen | His last words; she eats it in the epilogue: "Worst one." |
| Saki: the Queen's hands stop at four | Epilogue: "Go to five. I'm right here." |
| Mira can't finish "Signed, sealed..." after the fight | Finishes it when she forgives Kai, and for Sena at the end |
| Kai promises to ask first; Mira asks him to hold on | "Can I look after you?" / "Yes!"; "You said you'd hold on!" / "You said yes first." |
| Kai's teapots (eleven) | Rin buys them back at half price |

## When scenes play (app.rs)

- `prologue`: before the first visit to the story map.
- `wN_intro`: the first time world N is on the map.
- `wN_nK`: just before node K's match, the first time (short: 3-7 lines). Node 3 is preceded by
  `wN_mid` and node 6 by `wN_boss` (the long turning points), then the node's own scene if any.
- `wN_nK_win` / `wN_nK_lose`: after the first win and the first loss at node K (2-4 lines, the
  rival or Kai reacting). Node 6's win plays `wN_clear` instead.
- `w4_clear`, then `w4_fall`, then `epilogue`: after first beating Kaede.

Node indices in ids are 0-based (`w0_n1` is the second match of Quad Reef).

## Rules for every line

- Expressions: normal, smile, delighted, angry, sad, shocked, smug, sleepy.
- Every line must fit three rows of the dialogue box (`--story-check`). Prefer 60-120 characters.
- No narration; everything is spoken. No em dashes. Plain ASCII quotes and apostrophes.
- `RIGHT` lists who stands on the right; everyone else stands on the left. Voices stand nowhere.
- Map-card lines (in `content.rs`) must fit two rows, keep the node's mechanic tip, and add a story
  nudge.
