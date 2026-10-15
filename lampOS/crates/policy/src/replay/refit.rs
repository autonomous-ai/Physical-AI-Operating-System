//! The weights fitted again to each model's own answers (#871,
//! `lamp-policy-replay --again LINES --labels FILE --refit`), so that a
//! model judged by weights fitted to another's answers can be told apart
//! from a model that answers worse.
//!
//! From a run's lines, labelled: for each model, a logistic regression on
//! its seven answers, on whether the room was quiet, whether the lamp was in
//! conversation and whether its name was heard, as the day's weights were
//! fitted (`attention.rs`). It is fitted on half the labelled lines and
//! counted on the other half, [`HALVES`] times over, each time on halves
//! drawn the same way for every model ([`SEED`]); beside it, the choices the
//! run made with the day's weights, counted on the same halves. Then the
//! weights fitted on every labelled line, to be read, not used: nothing
//! here changes what the lamp does.
//!
//! The day's weights are two sums, one for a lamp listening for its name and
//! one for a lamp not; this is one sum for every line, the name heard as one
//! more term, not heard and not known both counted as not heard. With a few
//! hundred labels there are not enough for two.
use super::Should;
use crate::attention::{self, Choice, Room};
use serde_json::{Map, Value, json};

/// How many times the labelled lines are halved, one half fitted and the
/// other kept back to count on.
pub const HALVES: usize = 200;
/// Where the halves are drawn from: the same for every model and every run,
/// so that two models are counted on the same lines.
pub const SEED: u64 = 871;
/// How strongly the weights, not where the sum starts, are pulled towards
/// nothing: scikit-learn's default (`C = 1`). Without it, an answer that
/// splits the labels perfectly would take its weight to no end.
pub const PULL: f64 = 1.0;
/// Fewer labelled lines with answers than this, and a model is not fitted.
pub const FEWEST: usize = 20;
/// The most a lamp answering what was not for it may be made to cost, as
/// many times an ignored request (`--cost-ratio`).
pub const MOST_COST: f64 = 20.0;

/// What the sum is made from after where it starts, in order.
pub fn terms() -> Vec<String> {
    attention::questions()
        .into_iter()
        .map(|(name, _)| name)
        .chain(["quiet", "in_conversation", "name_heard"].map(str::to_owned))
        .collect()
}

/// The seven questions' answers, then the room, the conversation and the
/// name.
const QUESTIONS: usize = 7;
const TERMS: usize = QUESTIONS + 3;
/// Where the sum starts, and then each term's weight.
type Weights = [f64; TERMS + 1];

/// One labelled line, for one model.
#[derive(Clone, Debug)]
struct Example {
    /// The line's place among the run's labelled lines.
    place: usize,
    terms: [f64; TERMS],
    should: Should,
    /// The choice the run made for this model, with the day's weights.
    chose: String,
    named: bool,
    busy: bool,
    other_language: bool,
}

fn example(line: &Value, judge: &Value, place: usize, names: &[String]) -> Option<Example> {
    let should = match line["should"].as_str()? {
        "answer" => Should::Answer,
        "stay_quiet" => Should::StayQuiet,
        _ => return None,
    };
    let mut terms = [0.0; TERMS];
    // The seven answers, by the questions' names; the three after them are
    // the line's own.
    for (term, name) in terms.iter_mut().zip(&names[..QUESTIONS]) {
        let yes = judge["answers"][name].as_f64()?;
        if !(0.0..=1.0).contains(&yes) {
            return None;
        }
        *term = yes;
    }
    terms[QUESTIONS] = f64::from(u8::from(line["room"] == json!(Room::Quiet)));
    terms[QUESTIONS + 1] = f64::from(u8::from(line["in_conversation"] == true));
    terms[QUESTIONS + 2] = f64::from(u8::from(line["name_heard"] == true));
    Some(Example {
        place,
        terms,
        should,
        chose: judge["choice"].as_str()?.to_owned(),
        named: line["names_lamp"] == true,
        busy: line["room"] == json!(Room::Busy),
        other_language: line["other_language"] == true,
    })
}

fn logistic(sum: f64) -> f64 {
    1.0 / (1.0 + (-sum).exp())
}

fn probability(weights: &Weights, terms: &[f64; TERMS]) -> f64 {
    logistic(
        weights[0]
            + weights[1..]
                .iter()
                .zip(terms)
                .map(|(weight, term)| weight * term)
                .sum::<f64>(),
    )
}

/// The weights that make `examples` most likely, less [`PULL`] times half
/// the weights' squares: Newton's steps, each solved exactly. `None` when
/// the examples are all of one kind, which leaves where the sum starts with
/// no end. Each example to be left alone counts `cost` times one to be
/// answered (#871, the first builder: answering what was not for the lamp
/// costs more than ignoring what was), 1 for none.
fn fit(examples: &[&Example], cost: f64) -> Option<Weights> {
    let answers = examples
        .iter()
        .filter(|example| example.should == Should::Answer)
        .count();
    if answers == 0 || answers == examples.len() {
        return None;
    }
    let mut weights: Weights = [0.0; TERMS + 1];
    for _ in 0..100 {
        let mut slope = [0.0; TERMS + 1];
        let mut curve = [[0.0; TERMS + 1]; TERMS + 1];
        for example in examples {
            let mut x = [1.0; TERMS + 1];
            x[1..].copy_from_slice(&example.terms);
            let p = probability(&weights, &example.terms);
            let y = f64::from(u8::from(example.should == Should::Answer));
            let counts = if example.should == Should::StayQuiet {
                cost
            } else {
                1.0
            };
            for i in 0..=TERMS {
                slope[i] += counts * (p - y) * x[i];
                for j in 0..=TERMS {
                    curve[i][j] += counts * p * (1.0 - p) * x[i] * x[j];
                }
            }
        }
        for i in 1..=TERMS {
            slope[i] += PULL * weights[i];
            curve[i][i] += PULL;
        }
        let step = solve(curve, slope)?;
        for (weight, step) in weights.iter_mut().zip(step) {
            *weight -= step;
        }
        if step.iter().all(|step| step.abs() < 1e-10) {
            break;
        }
    }
    weights
        .iter()
        .all(|weight| weight.is_finite())
        .then_some(weights)
}

/// `a x = b` by elimination, the largest left in each column taken first.
fn solve(mut a: [[f64; TERMS + 1]; TERMS + 1], mut b: [f64; TERMS + 1]) -> Option<Weights> {
    let n = TERMS + 1;
    for column in 0..n {
        let pivot =
            (column..n).max_by(|&i, &j| a[i][column].abs().total_cmp(&a[j][column].abs()))?;
        if a[pivot][column].abs() < 1e-12 {
            return None;
        }
        a.swap(column, pivot);
        b.swap(column, pivot);
        for row in column + 1..n {
            let (above, below) = a.split_at_mut(row);
            let (pivot, target) = (&above[column], &mut below[0]);
            let factor = target[column] / pivot[column];
            for (cell, from) in target.iter_mut().zip(pivot).skip(column) {
                *cell -= factor * from;
            }
            b[row] -= factor * b[column];
        }
    }
    let mut x = [0.0; TERMS + 1];
    for row in (0..n).rev() {
        let rest: f64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - rest) / a[row][row];
    }
    Some(x)
}

/// SplitMix64: the same numbers from the same seed on every machine.
struct Draw(u64);

impl Draw {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// `0..n` in an order drawn from the seed.
    fn order(&mut self, n: usize) -> Vec<usize> {
        let mut order: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            let j = (self.next() % (i as u64 + 1)) as usize;
            order.swap(i, j);
        }
        order
    }
}

/// Counts over the kept-back halves, summed: divided by the halves counted
/// when told.
#[derive(Default)]
struct Counts {
    halves: usize,
    labelled: [usize; 4],
    right: [usize; 4],
    answered_not_for_it: usize,
    ignored_for_it: usize,
}

impl Counts {
    fn count(&mut self, example: &Example, took_up: bool) {
        let right = took_up == (example.should == Should::Answer);
        for (group, within) in [true, example.named, example.busy, example.other_language]
            .into_iter()
            .enumerate()
        {
            if within {
                self.labelled[group] += 1;
                self.right[group] += usize::from(right);
            }
        }
        match (example.should, took_up) {
            (Should::StayQuiet, true) => self.answered_not_for_it += 1,
            (Should::Answer, false) => self.ignored_for_it += 1,
            _ => {}
        }
    }

    /// The means over the halves, and what the mistakes cost with `cost`
    /// for each answer that was not for the lamp and 1 for each request
    /// ignored.
    fn told(&self, cost: f64) -> Value {
        let mean_of = |sum: f64| (sum / self.halves.max(1) as f64 * 10.0).round() / 10.0;
        let mean = |sum: usize| mean_of(sum as f64);
        let group = |index: usize| json!({"labelled": mean(self.labelled[index]), "right": mean(self.right[index])});
        json!({
            "right": group(0),
            "right_named": group(1),
            "right_busy": group(2),
            "right_other_language": group(3),
            "answered_not_for_the_lamp": mean(self.answered_not_for_it),
            "ignored_for_the_lamp": mean(self.ignored_for_it),
            "cost": mean_of(cost * self.answered_not_for_it as f64 + self.ignored_for_it as f64),
        })
    }
}

/// The refit's counts for every model in `lines` (an `--again` run's,
/// labelled): one JSON line, no words. Every mistake costs the same.
pub fn summary(lines: &[Value], models: &[String]) -> Value {
    summary_costing(lines, models, 1.0)
}

/// The same with an answer that was not for the lamp costing `cost` times
/// an ignored request, in the fit and in the counts' `cost` (#871).
pub fn summary_costing(lines: &[Value], models: &[String], cost: f64) -> Value {
    let names = terms();
    let labelled: Vec<&Value> = lines
        .iter()
        .filter(|line| line["should"].is_string())
        .collect();
    let mut draw = Draw(SEED);
    let halves: Vec<Vec<usize>> = (0..HALVES).map(|_| draw.order(labelled.len())).collect();
    let mut each = Map::new();
    for (index, model) in models.iter().enumerate() {
        let examples: Vec<Example> = labelled
            .iter()
            .enumerate()
            .filter_map(|(place, line)| example(line, &line["judges"][index], place, &names))
            .collect();
        let without_answers = labelled.len() - examples.len();
        // Not fitted, and why: no counts are given then, so that nothing
        // measured is taken from a fit that was never made (the reviewer
        // on #873).
        let unfitted_because = |why: String| {
            json!({"labelled": labelled.len(), "without_answers": without_answers,
                   "fitted": false, "why_not": why})
        };
        if examples.len() < FEWEST {
            let why = format!("fewer than {FEWEST} labelled lines with answers");
            each.insert(model.clone(), unfitted_because(why));
            continue;
        }
        let answers = examples
            .iter()
            .filter(|example| example.should == Should::Answer)
            .count();
        if answers == 0 || answers == examples.len() {
            let why = "its labelled lines with answers are all of one kind".to_owned();
            each.insert(model.clone(), unfitted_because(why));
            continue;
        }
        let (mut day, mut refitted) = (Counts::default(), Counts::default());
        let mut unfitted = 0;
        for order in &halves {
            // The first half of the order is fitted on, the rest kept back.
            let mut fitted_on = vec![false; labelled.len()];
            for place in &order[..labelled.len() / 2] {
                fitted_on[*place] = true;
            }
            let (on, back): (Vec<&Example>, Vec<&Example>) = examples
                .iter()
                .partition(|example| fitted_on[example.place]);
            let Some(weights) = fit(&on, cost) else {
                unfitted += 1;
                continue;
            };
            day.halves += 1;
            refitted.halves += 1;
            for example in back {
                day.count(example, example.chose != Choice::StayQuiet.as_str());
                let chosen = Choice::at(probability(&weights, &example.terms) as f32);
                refitted.count(example, chosen != Choice::StayQuiet);
            }
        }
        let all: Vec<&Example> = examples.iter().collect();
        let Some(weights) = fit(&all, cost) else {
            let why = "the weights could not be fitted on every label".to_owned();
            each.insert(model.clone(), unfitted_because(why));
            continue;
        };
        if day.halves == 0 {
            let why = "no half could be fitted: each held labels of one kind only".to_owned();
            each.insert(model.clone(), unfitted_because(why));
            continue;
        }
        let mut named = Map::new();
        named.insert("start".into(), json!(round(weights[0])));
        for (name, weight) in names.iter().zip(&weights[1..]) {
            named.insert(name.clone(), json!(round(*weight)));
        }
        each.insert(
            model.clone(),
            json!({
                "labelled": labelled.len(),
                "without_answers": without_answers,
                "fitted": true,
                // The counts are means over the halves fitted only.
                "halves_fitted": day.halves,
                "halves_unfitted": unfitted,
                "day_weights": day.told(cost),
                "refitted": refitted.told(cost),
                "weights_on_every_label": Value::Object(named),
            }),
        );
    }
    json!({
        "status": "refit_summary",
        "terms": names,
        "halves": HALVES,
        "seed": SEED,
        "pull": PULL,
        "cost_ratio": cost,
        "counted": "mean over the halves kept back; `answer` and `look_up` both take the person up; `cost` is cost_ratio for each answer not for the lamp and 1 for each request ignored",
        "models": Value::Object(each),
        "words": "none: counts and weights only",
    })
}

fn round(value: f64) -> f64 {
    (value * 1_000.0).round() / 1_000.0
}

#[cfg(test)]
mod tests;
