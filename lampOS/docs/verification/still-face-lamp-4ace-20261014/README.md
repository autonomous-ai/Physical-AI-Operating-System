# lamp-4ace's faces replayed through `--leave-still-faces` (#763, #766)

Numbers only: no picture, no name.

- `faces.csv`: every face lamp-4ace's eyes published on `humans/faces/tracked`
  from 2026-10-14 18:23 to 2026-10-15 13:17 (+07), 11,888 rows, in order.
  Columns `at_unix_ms`, `xmin`, `ymin`, `xmax`, `ymax` (shares of the
  picture), `confidence`. Taken from the first builder's recording
  (`first/faces-lamp-4ace-20261014`, `faces-day-20261014.csv.gz`), keeping
  only the rows with a face and only the columns the filter reads.
- The replay is the host test
  `still::tests::lamp_4ace_s_night_is_left_out_and_its_people_are_kept`
  (`cargo test -p lamp-eyes --lib still -- --nocapture` prints the counts).

## What the recording shows

| | Faces | Height (5% / 50% / 95%) | Score (5% / 50% / 95%) |
|---|---|---|---|
| The false spot (centre 0.62–0.78 across, 0.52–0.68 down) | 595 | 0.090 / 0.105 / 0.123 | 0.60 / 0.64 / 0.74 |
| People at the picture's edge (centre 0.9 across or more) | 390 | 0.058 / 0.099 / 0.114 | 0.62 / 0.83 / 0.88 |
| The morning at the desk (10:00 to 13:59) | 10,853 | 0.093 / 0.202 / 0.283 | 0.66 / 0.87 / 0.92 |

The false face's box wanders 0.01 to 0.02 of the picture look to look (its
edges' spread is 0.012 to 0.032), as much as a person at the desk, and it
sits at two heights (top edge about 0.52 or about 0.59). A stillness rule
cannot separate it from people:

| Rule, replayed | Night (55) kept | False spot (595) kept | Edge (390) kept | All (11,888) kept |
|---|---|---|---|---|
| Every edge within 0.007 for 2 min (the first draft) | 51 | 581 | 390 | 11,874 |
| Every edge within 0.03 for 2 min | 15 | 391 | 387 | 11,641 |
| Centre within 0.02 for 5 min | 20 | 134 | 384 | 7,476 |

## The rule shipped, replayed

A place (centre and height within 0.03) that has given faces for half an
hour, ten or more, none 0.14 of the height or taller and none scored 0.8 or
more; forgotten after two hours unseen; 32 places at most.

| | Seen | Left out |
|---|---|---|
| Night, 01:00 to 05:59 | 55 | 55 |
| False spot, the rest of the time | 540 | 448 |
| People at the edge | 390 | 0 |
| Morning at the desk | 10,853 | 0 |
| All | 11,888 | 505 |

## Raising `LEAST_SCORE` (0.6) instead

| Least score | False spot dropped (of 595) | Edge dropped (of 390) | Desk dropped (of 10,853) |
|---|---|---|---|
| 0.65 | 320 | 40 | 411 |
| 0.70 | 494 | 71 | 916 |
| 0.75 | 574 | 105 | 1,526 |
| 0.80 | 595 | 160 | 2,398 |

It removes the false face only by dropping real ones too.
