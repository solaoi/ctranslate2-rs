// translator.rs
//
// Copyright (c) 2023-2024 Junpei Kawamoto
//
// This software is released under the MIT License.
//
// http://opensource.org/licenses/mit-license.php

//! This module provides a Rust binding to the
//! [`ctranslate2::Translator`](https://opennmt.net/CTranslate2/python/ctranslate2.Translator.html).
//!
//! The main structure provided by this module is the [`Translator`] structure, which serves as
//! the interface to the translation functionalities of the `ctranslate2` library.
//!
//! In addition to the `Translator`, this module also offers various supportive structures such
//! as [`TranslationOptions`] and [`TranslationResult`].
//!
//! For more detailed information on each structure and its usage, please refer to their respective
//! documentation within this module.

use std::path::Path;

use anyhow::{anyhow, Error, Result};
use cxx::UniquePtr;

use crate::config::{BatchType, Config};
pub use crate::types::ffi::GenerationStepResult;
use crate::types::vec_ffi_vecstr;

trait GenerationCallback {
    fn execute(&mut self, res: GenerationStepResult) -> bool;
}

impl<F: FnMut(GenerationStepResult) -> bool> GenerationCallback for F {
    fn execute(&mut self, args: GenerationStepResult) -> bool {
        self(args)
    }
}
type TranslationCallbackBox<'a> = Box<dyn GenerationCallback + 'a>;

impl<'a> From<Option<&'a mut dyn FnMut(GenerationStepResult) -> bool>>
    for TranslationCallbackBox<'a>
{
    fn from(opt: Option<&'a mut dyn FnMut(GenerationStepResult) -> bool>) -> Self {
        match opt {
            None => Box::new(|_| false) as TranslationCallbackBox,
            Some(c) => Box::new(c) as TranslationCallbackBox,
        }
    }
}

fn execute_translation_callback(f: &mut TranslationCallbackBox, arg: GenerationStepResult) -> bool {
    f.execute(arg)
}

#[cxx::bridge]
mod ffi {
    struct TranslationOptions<'a> {
        beam_size: usize,
        patience: f32,
        length_penalty: f32,
        coverage_penalty: f32,
        repetition_penalty: f32,
        no_repeat_ngram_size: usize,
        disable_unk: bool,
        suppress_sequences: Vec<VecStr<'a>>,
        prefix_bias_beta: f32,
        // end_token,
        return_end_token: bool,
        max_input_length: usize,
        max_decoding_length: usize,
        min_decoding_length: usize,
        sampling_topk: usize,
        sampling_topp: f32,
        sampling_temperature: f32,
        use_vmap: bool,
        num_hypotheses: usize,
        return_scores: bool,
        return_attention: bool,
        return_alternatives: bool,
        min_alternative_expansion_prob: f32,
        replace_unknowns: bool,
        max_batch_size: usize,
        batch_type: BatchType,
    }

    struct TranslationResult {
        hypotheses: Vec<VecString>,
        scores: Vec<f32>,
        // attention: Vec<Vec<Vec<f32>>>,
    }

    extern "Rust" {
        type TranslationCallbackBox<'a>;
        fn execute_translation_callback(
            f: &mut TranslationCallbackBox,
            arg: GenerationStepResult,
        ) -> bool;
    }

    unsafe extern "C++" {
        include!("ct2rs/src/types.rs.h");
        include!("ct2rs/include/translator.h");

        type VecString = crate::types::ffi::VecString;
        type VecStr<'a> = crate::types::ffi::VecStr<'a>;

        type Config = crate::config::ffi::Config;
        type BatchType = crate::config::ffi::BatchType;
        type GenerationStepResult = crate::types::ffi::GenerationStepResult;

        type Translator;

        fn translator(model_path: &str, config: UniquePtr<Config>)
            -> Result<UniquePtr<Translator>>;

        fn translate_batch(
            self: &Translator,
            source: &Vec<VecStr>,
            options: &TranslationOptions,
            has_callback: bool,
            callback: &mut TranslationCallbackBox,
        ) -> Result<Vec<TranslationResult>>;

        fn translate_batch_with_target_prefix(
            self: &Translator,
            source: &Vec<VecStr>,
            target_prefix: &Vec<VecStr>,
            options: &TranslationOptions,
            has_callback: bool,
            callback: &mut TranslationCallbackBox,
        ) -> Result<Vec<TranslationResult>>;

        fn num_queued_batches(self: &Translator) -> Result<usize>;

        fn num_active_batches(self: &Translator) -> Result<usize>;

        fn num_replicas(self: &Translator) -> Result<usize>;
    }
}

unsafe impl Send for ffi::Translator {}
unsafe impl Sync for ffi::Translator {}

/// Options for translation.
#[derive(Clone, Debug)]
pub struct TranslationOptions<T: AsRef<str>> {
    /// Beam size to use for beam search (set 1 to run greedy search).
    pub beam_size: usize,
    /// Beam search patience factor, as described in <https://arxiv.org/abs/2204.05424>.
    /// The decoding will continue until beam_size*patience hypotheses are finished.
    pub patience: f32,
    /// Exponential penalty applied to the length during beam search.
    /// The scores are normalized with:
    ///   hypothesis_score /= (hypothesis_length ** length_penalty)
    pub length_penalty: f32,
    /// Coverage penalty weight applied during beam search.
    pub coverage_penalty: f32,
    /// Penalty applied to the score of previously generated tokens, as described in
    /// <https://arxiv.org/abs/1909.05858> (set > 1 to penalize).
    pub repetition_penalty: f32,
    /// Prevent repetitions of ngrams with this size (set 0 to disable).
    pub no_repeat_ngram_size: usize,
    /// Disable the generation of the unknown token.
    pub disable_unk: bool,
    /// Disable the generation of some sequences of tokens.
    pub suppress_sequences: Vec<Vec<T>>,
    /// Biases decoding towards a given prefix, see <https://arxiv.org/abs/1912.03393> --section 4.2
    /// Only activates biased-decoding when beta is in range (0, 1) and SearchStrategy is set to BeamSearch.
    /// The closer beta is to 1, the stronger the bias is towards the given prefix.
    ///
    /// If beta <= 0 and a non-empty prefix is given, then the prefix will be used as a
    /// hard-prefix rather than a soft, biased-prefix.
    pub prefix_bias_beta: f32,
    // Stop the decoding on one of these tokens (defaults to the model EOS token).
    // end_token,
    /// Include the end token in the result.
    pub return_end_token: bool,
    /// Truncate the inputs after this many tokens (set 0 to disable truncation).
    pub max_input_length: usize,
    /// Decoding length constraints.
    pub max_decoding_length: usize,
    /// Decoding length constraints.
    pub min_decoding_length: usize,
    /// Randomly sample from the top K candidates (set 0 to sample from the full output distribution).
    pub sampling_topk: usize,
    /// Keep the most probable tokens whose cumulative probability exceeds this value.
    pub sampling_topp: f32,
    /// High temperature increase randomness.
    pub sampling_temperature: f32,
    /// Allow using the vocabulary map included in the model directory, if it exists.
    pub use_vmap: bool,
    /// Number of hypotheses to store in the TranslationResult class.
    pub num_hypotheses: usize,
    /// Store scores in the TranslationResult class.
    pub return_scores: bool,
    /// Store attention vectors in the TranslationResult class.
    pub return_attention: bool,
    /// Return alternatives at the first unconstrained decoding position. This is typically
    /// used with a target prefix to provide alternatives at a specifc location in the
    /// translation.
    pub return_alternatives: bool,
    /// Minimum probability to expand an alternative.
    pub min_alternative_expansion_prob: f32,
    /// Replace unknown target tokens by the original source token with the highest attention.
    pub replace_unknowns: bool,
    /// The maximum batch size. If the number of inputs is greater than `max_batch_size`,
    /// the inputs are sorted by length and split by chunks of `max_batch_size` examples
    /// so that the number of padding positions is minimized.
    pub max_batch_size: usize,
    /// Whether `max_batch_size` is the number of “examples” or “tokens”.
    pub batch_type: BatchType,
}

impl Default for TranslationOptions<String> {
    fn default() -> Self {
        Self {
            beam_size: 2,
            patience: 1.,
            length_penalty: 1.,
            coverage_penalty: 0.,
            repetition_penalty: 1.,
            no_repeat_ngram_size: 0,
            disable_unk: false,
            suppress_sequences: vec![],
            prefix_bias_beta: 0.,
            return_end_token: false,
            max_input_length: 1024,
            max_decoding_length: 256,
            min_decoding_length: 1,
            sampling_topk: 1,
            sampling_topp: 1.,
            sampling_temperature: 1.,
            use_vmap: false,
            num_hypotheses: 1,
            return_scores: false,
            return_attention: false,
            return_alternatives: false,
            min_alternative_expansion_prob: 0.,
            replace_unknowns: false,
            max_batch_size: 0,
            batch_type: BatchType::default(),
        }
    }
}

impl<T: AsRef<str>> TranslationOptions<T> {
    fn to_ffi(&self) -> ffi::TranslationOptions {
        ffi::TranslationOptions {
            beam_size: self.beam_size,
            patience: self.patience,
            length_penalty: self.length_penalty,
            coverage_penalty: self.coverage_penalty,
            repetition_penalty: self.repetition_penalty,
            no_repeat_ngram_size: self.no_repeat_ngram_size,
            disable_unk: self.disable_unk,
            suppress_sequences: vec_ffi_vecstr(self.suppress_sequences.as_ref()),
            prefix_bias_beta: self.prefix_bias_beta,
            return_end_token: self.return_end_token,
            max_input_length: self.max_input_length,
            max_decoding_length: self.max_decoding_length,
            min_decoding_length: self.min_decoding_length,
            sampling_topk: self.sampling_topk,
            sampling_topp: self.sampling_topp,
            sampling_temperature: self.sampling_temperature,
            use_vmap: self.use_vmap,
            num_hypotheses: self.num_hypotheses,
            return_scores: self.return_scores,
            return_attention: self.return_attention,
            return_alternatives: self.return_alternatives,
            min_alternative_expansion_prob: self.min_alternative_expansion_prob,
            replace_unknowns: self.replace_unknowns,
            max_batch_size: self.max_batch_size,
            batch_type: self.batch_type,
        }
    }
}

/// A Rust binding to the
/// [`ctranslate2::Translator`](https://opennmt.net/CTranslate2/python/ctranslate2.Translator.html).
///
/// # Example
/// Below is an example where a given list of tokens is translated:
///
/// ```no_run
/// # use anyhow::Result;
/// use ct2rs::config::{Config, Device};
/// use ct2rs::translator::Translator;
///
/// # fn main() -> Result<()> {
/// let translator = Translator::new("/path/to/model", &Config::default())?;
/// let res = translator.translate_batch(
///     &vec![vec!["▁Hello", "▁world", "!", "</s>", "<unk>"]],
///     &Default::default(),
///     None,
/// )?;
/// for r in res {
///     println!("{:?}", r);
/// }
/// # Ok(())
/// # }
/// ```
///
/// If the model requires target prefixes, use [`Translator::translate_batch_with_target_prefix`]
/// instead:
///
/// ```no_run
/// # use anyhow::Result;
/// use ct2rs::config::{Config, Device};
/// use ct2rs::translator::Translator;
///
/// # fn main() -> Result<()> {
/// let translator = Translator::new("/path/to/model", &Config::default())?;
/// let res = translator.translate_batch_with_target_prefix(
///     &vec![vec!["▁Hello", "▁world", "!", "</s>", "<unk>"]],
///     &vec![vec!["jpn_Jpan"]],
///     &Default::default(),
///     None,
/// )?;
/// for r in res {
///     println!("{:?}", r);
/// }
/// # Ok(())
/// # }
/// ```
pub struct Translator {
    ptr: UniquePtr<ffi::Translator>,
}

impl Translator {
    /// Creates and initializes an instance of `Translator`.
    ///
    /// This function constructs a new `Translator` by loading a language model from the specified
    /// `model_path` and applying the provided `config` settings.
    ///
    /// # Arguments
    /// * `model_path` - A path to the directory containing the language model to be loaded.
    /// * `config` - A reference to a `Config` structure that specifies various settings
    ///   and configurations for the `Translator`.
    ///
    /// # Returns
    /// Returns a `Result` that, if successful, contains the initialized `Translator`. If an error
    /// occurs during initialization, the function will return an error wrapped in the `Result`.
    ///
    /// # Example
    /// ```no_run
    /// use ct2rs::config::Config;
    /// use ct2rs::translator::Translator;
    ///
    /// let config = Config::default();
    /// let translator = Translator::new("/path/to/model", &config)
    ///     .expect("Failed to create translator");
    /// ```
    pub fn new<T: AsRef<Path>>(model_path: T, config: &Config) -> Result<Translator> {
        Ok(Translator {
            ptr: ffi::translator(
                model_path
                    .as_ref()
                    .to_str()
                    .ok_or(anyhow!("invalid path: {}", model_path.as_ref().display()))?,
                config.to_ffi(),
            )?,
        })
    }

    /// Translates multiple lists of tokens in a batch processing manner.
    ///
    /// This function takes a vector of token lists and performs batch translation according to the
    /// specified settings in `options`. The results of the batch translation are returned as a
    /// vector. An optional `callback` closure can be provided which is invoked for each new token
    /// generated during the translation process. This allows for step-by-step reception of the
    /// batch translation results. If the callback returns `true`, it will stop the translation for
    /// that batch. Note that if a callback is provided, `options.beam_size` must be set to `1`.
    ///
    /// # Arguments
    /// * `source` - A vector of token lists, each list representing a sequence of tokens to be
    ///    translated.
    /// * `options` - Settings applied to the batch translation process.
    /// * `callback` - An optional mutable reference to a closure that is called for each token
    ///   generation step. The closure takes a `GenerationStepResult` and returns a `bool`. If it
    ///   returns `true`, the translation process for the current batch will stop.
    ///
    /// # Returns
    /// Returns a `Result` containing a vector of `TranslationResult` if successful, or an error if
    /// the translation fails.
    ///
    /// # Example
    /// ```no_run
    /// use ct2rs::config::Config;
    /// use ct2rs::translator::{Translator, TranslationOptions, GenerationStepResult};
    ///
    /// let source_tokens = vec![
    ///     vec!["▁Hall", "o", "▁World", "!", "</s>"],
    ///     vec![
    ///         "▁This", "▁library", "▁is", "▁a", "▁", "Rust", "▁", "binding", "s", "▁of",
    ///         "▁C", "Trans", "late", "2", ".", "</s>"
    ///     ],
    /// ];
    /// let options = TranslationOptions::default();
    /// let mut callback = |step_result: GenerationStepResult| -> bool {
    ///     println!("{:?}", step_result);
    ///     false // Continue processing
    /// };
    /// let translator = Translator::new("/path/to/model", &Config::default())
    ///     .expect("Failed to create translator");
    /// let results = translator.translate_batch(&source_tokens, &options, Some(&mut callback))
    ///     .expect("Translation failed");
    /// ```
    pub fn translate_batch<'a, T, V>(
        &self,
        source: &Vec<Vec<T>>,
        options: &TranslationOptions<V>,
        callback: Option<&'a mut dyn FnMut(GenerationStepResult) -> bool>,
    ) -> Result<Vec<TranslationResult>>
    where
        T: AsRef<str> + std::fmt::Debug,
        V: AsRef<str>,
    {
        println!("{:?}", source);
        Ok(self
            .ptr
            .translate_batch(
                &vec_ffi_vecstr(source),
                &options.to_ffi(),
                callback.is_some(),
                &mut TranslationCallbackBox::from(callback),
            )?
            .into_iter()
            .map(TranslationResult::from)
            .collect())
    }

    /// Translates multiple lists of tokens with target prefixes in a batch processing manner.
    ///
    /// This function takes a vector of token lists and corresponding target prefixes, performing
    /// batch translation according to the specified settings in `options`. An optional `callback`
    /// closure can be provided which is invoked for each new token generated during the translation
    /// process.
    ///
    /// This function is similar to `translate_batch`, with the addition of handling target prefixes
    /// that guide the translation process. For more detailed parameter and option descriptions,
    /// refer to the documentation for [`Translator::translate_batch`].
    ///
    /// # Arguments
    /// * `source` - A vector of token lists, each list representing a sequence of tokens to be
    ///   translated.
    /// * `target_prefix` - A vector of token lists, each list representing a sequence of target
    ///   prefix tokens that provide a starting point for the translation output.
    /// * `options` - Settings applied to the batch translation process.
    /// * `callback` - An optional mutable reference to a closure that is called for each token
    ///   generation step. The closure takes a `GenerationStepResult` and returns a `bool`. If it
    ///   returns `true`, the translation process for the current batch will stop.
    ///
    /// # Returns
    /// Returns a `Result` containing a vector of `TranslationResult` if successful, or an error if
    /// the translation fails.
    pub fn translate_batch_with_target_prefix<'a, T, U, V>(
        &self,
        source: &Vec<Vec<T>>,
        target_prefix: &Vec<Vec<U>>,
        options: &TranslationOptions<V>,
        callback: Option<&'a mut dyn FnMut(GenerationStepResult) -> bool>,
    ) -> Result<Vec<TranslationResult>>
    where
        T: AsRef<str>,
        U: AsRef<str>,
        V: AsRef<str>,
    {
        Ok(self
            .ptr
            .translate_batch_with_target_prefix(
                &vec_ffi_vecstr(source),
                &vec_ffi_vecstr(target_prefix),
                &options.to_ffi(),
                callback.is_some(),
                &mut TranslationCallbackBox::from(callback),
            )?
            .into_iter()
            .map(TranslationResult::from)
            .collect())
    }

    /// Number of batches in the work queue.
    #[inline]
    pub fn num_queued_batches(&self) -> Result<usize> {
        self.ptr.num_queued_batches().map_err(Error::from)
    }

    /// Number of batches in the work queue or currently processed by a worker.
    #[inline]
    pub fn num_active_batches(&self) -> Result<usize> {
        self.ptr.num_active_batches().map_err(Error::from)
    }

    /// Number of parallel replicas.
    #[inline]
    pub fn num_replicas(&self) -> Result<usize> {
        self.ptr.num_replicas().map_err(Error::from)
    }
}

/// A translation result.
#[derive(Clone, Debug)]
pub struct TranslationResult {
    /// Translation hypotheses.
    pub hypotheses: Vec<Vec<String>>,
    /// Score of each translation hypothesis (empty if return_scores was disabled).
    pub scores: Vec<f32>,
}

impl From<ffi::TranslationResult> for TranslationResult {
    fn from(r: ffi::TranslationResult) -> Self {
        Self {
            hypotheses: r.hypotheses.into_iter().map(|h| h.v).collect(),
            scores: r.scores,
        }
    }
}

impl TranslationResult {
    /// Returns the first translation hypothesis if exists.
    #[inline]
    pub fn output(&self) -> Option<&Vec<String>> {
        self.hypotheses.first()
    }

    /// Returns the score of the first translation hypothesis if exists.
    #[inline]
    pub fn score(&self) -> Option<f32> {
        self.scores.first().copied()
    }

    /// Returns the number of translation hypotheses.
    #[inline]
    pub fn num_hypotheses(&self) -> usize {
        self.hypotheses.len()
    }

    /// Returns true if this result contains scores.
    #[inline]
    pub fn has_scores(&self) -> bool {
        !self.scores.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use crate::config::BatchType;
    use crate::translator::TranslationOptions;

    #[test]
    fn default_translation_options() {
        let options = TranslationOptions::default();

        assert_eq!(options.beam_size, 2);
        assert_eq!(options.patience, 1.);
        assert_eq!(options.length_penalty, 1.);
        assert_eq!(options.coverage_penalty, 0.);
        assert_eq!(options.repetition_penalty, 1.);
        assert_eq!(options.no_repeat_ngram_size, 0);
        assert!(!options.disable_unk);
        assert!(options.suppress_sequences.is_empty());
        assert_eq!(options.prefix_bias_beta, 0.);
        assert!(!options.return_end_token);
        assert_eq!(options.max_input_length, 1024);
        assert_eq!(options.max_decoding_length, 256);
        assert_eq!(options.min_decoding_length, 1);
        assert_eq!(options.sampling_topk, 1);
        assert_eq!(options.sampling_topp, 1.);
        assert_eq!(options.sampling_temperature, 1.);
        assert!(!options.use_vmap);
        assert_eq!(options.num_hypotheses, 1);
        assert!(!options.return_scores);
        assert!(!options.return_attention);
        assert!(!options.return_alternatives);
        assert_eq!(options.min_alternative_expansion_prob, 0.);
        assert!(!options.replace_unknowns);
        assert_eq!(options.max_batch_size, 0);
        assert_eq!(options.batch_type, BatchType::default());
    }
}
