import { JSX, Show } from "solid-js";
import { resetErrorBoundaries } from "solid-js";
import "./ErrorBoundary.css";

interface FallbackProps {
  error: any;
  reset: () => void;
}

function ErrorFallback(props: FallbackProps): JSX.Element {
  return (
    <div class="error-boundary">
      <div class="error-content">
        <h1>Something went wrong</h1>
        <p class="error-message">
          {props.error?.message || "An unexpected error occurred"}
        </p>
        <Show when={props.error?.stack}>
          <pre class="error-stack">{props.error?.stack}</pre>
        </Show>
        <div class="error-actions">
          <button
            class="error-button error-button-primary"
            onClick={() => {
              resetErrorBoundaries();
              props.reset();
              window.location.reload();
            }}
          >
            Reload Application
          </button>
          <button
            class="error-button error-button-secondary"
            onClick={() => props.reset()}
          >
            Try Again
          </button>
        </div>
        <p class="error-hint">
          If this problem persists, please check the console for more details or
          report the issue.
        </p>
      </div>
    </div>
  );
}

export default ErrorFallback;
