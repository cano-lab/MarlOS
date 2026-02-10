import { Component, createSignal, For, Show } from "solid-js";
import { useLearningSession, LearningStep, webSearch, academicSearch, formatSearchResultsForContext, formatAcademicResultsForContext } from "../hooks/useLearningSession";
import "./LearningPanel.css";

const PREDICTION_TYPES = [
  { id: "confident", label: "I'm fairly confident", emoji: "💪" },
  { id: "partial", label: "I have a partial idea", emoji: "🤔" },
  { id: "guess", label: "This is a guess", emoji: "🎲" },
  { id: "no_idea", label: "I don't know", emoji: "🌱" },
] as const;

const STEP_INFO: Record<LearningStep, { title: string; description: string }> = {
  topic: {
    title: "What do you want to learn?",
    description: "Ask a question or pick a topic to explore",
  },
  predict: {
    title: "Make a Prediction",
    description: "Before seeing the answer, what do you think? Any guess counts.",
  },
  answer: {
    title: "The Answer",
    description: "Here's what's actually true. Compare with your prediction.",
  },
  compare: {
    title: "Prediction Analysis",
    description: "Let's see what your prediction reveals about your mental model.",
  },
  integrate: {
    title: "What Did You Learn?",
    description: "Summarize the key insight in your own words.",
  },
  next: {
    title: "Keep Going",
    description: "Your updated model reveals new questions. Pick one or ask your own.",
  },
};

const LearningPanel: Component = () => {
  const {
    state,
    setTopic,
    submitPrediction,
    requestComparison,
    submitIntegration,
    startNewCycle,
    reset,
  } = useLearningSession();

  const [topicInput, setTopicInput] = createSignal("");
  const [predictionInput, setPredictionInput] = createSignal("");
  const [predictionType, setPredictionType] = createSignal<typeof PREDICTION_TYPES[number]["id"]>("guess");
  const [integrationInput, setIntegrationInput] = createSignal("");
  const [useWebSearch, setUseWebSearch] = createSignal(false);
  const [useAcademicSearch, setUseAcademicSearch] = createSignal(false);
  const [searchResults, setSearchResults] = createSignal<string>("");

  const handleTopicSubmit = async (e: Event) => {
    e.preventDefault();
    const topic = topicInput().trim();
    if (topic) {
      // Perform searches if enabled
      let searchContext = "";

      if (useWebSearch()) {
        const results = await webSearch(topic, 5);
        searchContext += formatSearchResultsForContext(results);
      }

      if (useAcademicSearch()) {
        const results = await academicSearch(topic, 5);
        searchContext += formatAcademicResultsForContext(results);
      }

      setSearchResults(searchContext);
      setTopic(topic, searchContext);
      setTopicInput("");
    }
  };

  const handlePredictionSubmit = (e: Event) => {
    e.preventDefault();
    const prediction = predictionInput().trim();
    if (prediction || predictionType() === "no_idea") {
      submitPrediction(
        prediction || "I don't know - please help me build a mental model",
        predictionType()
      );
      setPredictionInput("");
    }
  };

  const handleIntegrationSubmit = (e: Event) => {
    e.preventDefault();
    const integration = integrationInput().trim();
    if (integration) {
      submitIntegration(integration);
      setIntegrationInput("");
    }
  };

  const currentStep = () => state().step;
  const currentCycle = () => state().currentCycle;
  const isLoading = () => state().isLoading;
  const error = () => state().error;
  const history = () => state().history;

  return (
    <div class="learning-panel">
      {/* Header */}
      <div class="learning-header">
        <h2>Learning Mode</h2>
        <div class="learning-subtitle">
          Predict → Test → Compare → Integrate → Repeat
        </div>
        <Show when={history().length > 0}>
          <div class="cycle-count">{history().length} cycles completed</div>
        </Show>
      </div>

      {/* Progress indicator */}
      <div class="step-progress">
        <For each={Object.keys(STEP_INFO) as LearningStep[]}>
          {(step, index) => (
            <div
              class={`step-dot ${currentStep() === step ? "active" : ""} ${
                Object.keys(STEP_INFO).indexOf(currentStep()) > index() ? "complete" : ""
              }`}
              title={STEP_INFO[step].title}
            />
          )}
        </For>
      </div>

      {/* Current step info */}
      <div class="step-info">
        <h3>{STEP_INFO[currentStep()].title}</h3>
        <p>{STEP_INFO[currentStep()].description}</p>
      </div>

      {/* Error display */}
      <Show when={error()}>
        <div class="learning-error">{error()}</div>
      </Show>

      {/* Step content */}
      <div class="step-content">
        {/* TOPIC STEP */}
        <Show when={currentStep() === "topic"}>
          <form class="topic-form" onSubmit={handleTopicSubmit}>
            <textarea
              class="topic-input"
              placeholder="What do you want to understand? e.g., 'How does TCP handle packet loss?' or 'Explain quantum entanglement'"
              value={topicInput()}
              onInput={(e) => setTopicInput(e.currentTarget.value)}
              rows={3}
            />
            <div class="search-options">
              <label class="search-toggle">
                <input
                  type="checkbox"
                  checked={useWebSearch()}
                  onChange={(e) => setUseWebSearch(e.currentTarget.checked)}
                />
                <span class="toggle-icon">🌐</span>
                <span>Web Search</span>
              </label>
              <label class="search-toggle">
                <input
                  type="checkbox"
                  checked={useAcademicSearch()}
                  onChange={(e) => setUseAcademicSearch(e.currentTarget.checked)}
                />
                <span class="toggle-icon">📚</span>
                <span>Academic Papers</span>
              </label>
            </div>
            <button type="submit" class="submit-btn" disabled={!topicInput().trim()}>
              Start Learning
            </button>
          </form>
        </Show>

        {/* PREDICT STEP */}
        <Show when={currentStep() === "predict"}>
          <div class="predict-section">
            <div class="topic-display">
              <span class="label">Topic:</span>
              <span class="value">{currentCycle().topic}</span>
            </div>

            <Show when={searchResults()}>
              <div class="search-results-indicator">
                🔍 Web/Academic search results will enhance the answer
              </div>
            </Show>

            <form class="predict-form" onSubmit={handlePredictionSubmit}>
              {/* Prediction type selector */}
              <div class="prediction-type-selector">
                <For each={PREDICTION_TYPES}>
                  {(type) => (
                    <button
                      type="button"
                      class={`type-btn ${predictionType() === type.id ? "selected" : ""}`}
                      onClick={() => setPredictionType(type.id)}
                    >
                      <span class="emoji">{type.emoji}</span>
                      <span class="label">{type.label}</span>
                    </button>
                  )}
                </For>
              </div>

              <Show when={predictionType() !== "no_idea"}>
                <textarea
                  class="prediction-input"
                  placeholder="What do you think the answer is? It's okay to be wrong - that's where learning happens."
                  value={predictionInput()}
                  onInput={(e) => setPredictionInput(e.currentTarget.value)}
                  rows={4}
                />
              </Show>

              <Show when={predictionType() === "no_idea"}>
                <div class="no-idea-message">
                  <p>That's completely fine! Not knowing is the honest starting point.</p>
                  <p>I'll build you a minimal mental model so you can make predictions next time.</p>
                </div>
              </Show>

              <button
                type="submit"
                class="submit-btn"
                disabled={predictionType() !== "no_idea" && !predictionInput().trim()}
              >
                {isLoading() ? "Thinking..." : "Submit Prediction"}
              </button>
            </form>
          </div>
        </Show>

        {/* ANSWER STEP */}
        <Show when={currentStep() === "answer"}>
          <div class="answer-section">
            <div class="your-prediction">
              <span class="label">Your prediction:</span>
              <p class="prediction-text">{currentCycle().prediction}</p>
            </div>

            <div class="answer-display">
              <span class="label">The answer:</span>
              <div class="answer-content">{currentCycle().answer}</div>
            </div>

            <button
              class="submit-btn"
              onClick={requestComparison}
              disabled={isLoading()}
            >
              {isLoading() ? "Analyzing..." : "Analyze My Prediction"}
            </button>
          </div>
        </Show>

        {/* COMPARE STEP */}
        <Show when={currentStep() === "compare"}>
          <div class="compare-section">
            <div class="comparison-display">
              <div class="comparison-content">{currentCycle().comparison}</div>
            </div>

            <form class="integration-form" onSubmit={handleIntegrationSubmit}>
              <label class="integration-label">
                In your own words, what's the key thing you learned?
              </label>
              <textarea
                class="integration-input"
                placeholder="The main insight I'm taking away is..."
                value={integrationInput()}
                onInput={(e) => setIntegrationInput(e.currentTarget.value)}
                rows={3}
              />
              <button
                type="submit"
                class="submit-btn"
                disabled={!integrationInput().trim() || isLoading()}
              >
                {isLoading() ? "Generating questions..." : "Continue"}
              </button>
            </form>
          </div>
        </Show>

        {/* INTEGRATE/NEXT STEP */}
        <Show when={currentStep() === "integrate" || currentStep() === "next"}>
          <div class="next-section">
            <div class="integration-summary">
              <span class="label">What you learned:</span>
              <p>{currentCycle().integration}</p>
            </div>

            <Show when={currentCycle().followUpQuestions?.length}>
              <div class="followup-questions">
                <span class="label">Questions to explore next:</span>
                <div class="question-list">
                  <For each={currentCycle().followUpQuestions}>
                    {(question) => (
                      <button
                        class="question-btn"
                        onClick={() => startNewCycle(question)}
                      >
                        {question}
                      </button>
                    )}
                  </For>
                </div>
              </div>
            </Show>

            <div class="next-actions">
              <button class="secondary-btn" onClick={() => startNewCycle()}>
                Ask Something New
              </button>
            </div>
          </div>
        </Show>
      </div>

      {/* History sidebar (collapsed by default) */}
      <Show when={history().length > 0}>
        <details class="history-section">
          <summary>Learning History ({history().length} cycles)</summary>
          <div class="history-list">
            <For each={history()}>
              {(cycle) => (
                <div class="history-item">
                  <div class="history-topic">{cycle.topic}</div>
                  <div class="history-prediction">
                    Predicted: {cycle.prediction.slice(0, 50)}...
                  </div>
                  <button
                    class="revisit-btn"
                    onClick={() => startNewCycle(cycle.topic)}
                  >
                    Revisit
                  </button>
                </div>
              )}
            </For>
          </div>
        </details>
      </Show>

      {/* Reset button */}
      <Show when={currentStep() !== "topic"}>
        <button class="reset-btn" onClick={reset}>
          Start Over
        </button>
      </Show>
    </div>
  );
};

export default LearningPanel;
