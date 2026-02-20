import { Component, createSignal, onMount, onCleanup, For } from "solid-js";
import type { Widget } from "./PlanSpace";
import "./WeatherWidget.css";

interface WeatherWidgetProps {
  widget: Widget;
  onUpdate: (updates: Partial<Widget>) => void;
}

interface WeatherData {
  current: {
    temperature: number;
    code: number;
    is_day: number;
  };
  daily?: Array<{
    date: string;
    temperature_max: number;
    temperature_min: number;
    weather_code: number;
  }>;
}

const WIDGET_CODES: Record<number, { icon: string; label: string }> = {
  0: { icon: "☀️", label: "Clear sky" },
  1: { icon: "🌤", label: "Mainly clear" },
  2: { icon: "⛅", label: "Partly cloudy" },
  3: { icon: "☁️", label: "Overcast" },
  45: { icon: "🌫", label: "Fog" },
  48: { icon: "🌫", label: "Depositing rime fog" },
  51: { icon: "🌧", label: "Light drizzle" },
  53: { icon: "🌧", label: "Moderate drizzle" },
  55: { icon: "🌧", label: "Dense drizzle" },
  61: { icon: "🌧", label: "Slight rain" },
  63: { icon: "🌧", label: "Moderate rain" },
  65: { icon: "🌧", label: "Heavy rain" },
  71: { icon: "❄️", label: "Slight snow" },
  73: { icon: "❄️", label: "Moderate snow" },
  75: { icon: "❄️", label: "Heavy snow" },
  77: { icon: "🌨", label: "Snow grains" },
  80: { icon: "🌧", label: "Slight rain showers" },
  81: { icon: "🌧", label: "Moderate rain showers" },
  82: { icon: "🌧", label: "Violent rain showers" },
  85: { icon: "❄️", label: "Slight snow showers" },
  86: { icon: "❄️", label: "Heavy snow showers" },
  95: { icon: "⛈", label: "Thunderstorm" },
  96: { icon: "⛈", label: "Thunderstorm with hail" },
  99: { icon: "⛈", label: "Thunderstorm with heavy hail" },
};

const DEFAULT_LOCATION = "London";
const REFRESH_INTERVAL = 15 * 60 * 1000; // 15 minutes

// Popular cities for quick access
const POPULAR_CITIES = [
  "New York", "Los Angeles", "Chicago", "Houston", "Phoenix",
  "London", "Paris", "Berlin", "Rome", "Madrid",
  "Tokyo", "Sydney", "Toronto", "Vancouver", "Sudbury",
  "Mexico City", "Buenos Aires", "Mumbai", "Delhi", "Singapore",
  "Hong Kong", "Dubai", "Cairo", "Montreal", "Calgary"
];

const WeatherWidget: Component<WeatherWidgetProps> = (props) => {
  const [location, setLocation] = createSignal(DEFAULT_LOCATION);
  const [units, setUnits] = createSignal<"celsius" | "fahrenheit">("celsius");
  const [weather, setWeather] = createSignal<WeatherData | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [isEditing, setIsEditing] = createSignal(false);
  const [locationInput, setLocationInput] = createSignal(DEFAULT_LOCATION);

  onMount(() => {
    if (props.widget.data.type === "weather") {
      setLocation(props.widget.data.location || DEFAULT_LOCATION);
      setUnits(props.widget.data.units || "celsius");
    }
    fetchWeather();
    const interval = setInterval(fetchWeather, REFRESH_INTERVAL);

    onCleanup(() => {
      clearInterval(interval);
    });
  });

  const celsiusToFahrenheit = (c: number) => (c * 9/5) + 32;

  const fetchWeather = async () => {
    const loc = location();
    if (!loc) return;

    // Only show loading state if we have no data yet
    if (!weather()) {
      setLoading(true);
    }
    setError(null);

    try {
      // First, geocode the location name to coordinates
      const geoUrl = `https://geocoding-api.open-meteo.com/v1/search?name=${encodeURIComponent(loc)}&count=1&language=en&format=json`;
      const geoResponse = await fetch(geoUrl);

      if (!geoResponse.ok) {
        throw new Error(`Geocoding failed: ${geoResponse.status}`);
      }

      const geoData = await geoResponse.json();

      if (!geoData.results || geoData.results.length === 0) {
        throw new Error("City not found. Try a larger nearby city.");
      }

      const { latitude, longitude, name, country, admin1 } = geoData.results[0];

      // Fetch weather data with daily forecast
      const weatherUrl = `https://api.open-meteo.com/v1/forecast?latitude=${latitude}&longitude=${longitude}&current=temperature_2m,weather_code,is_day&daily=weather_code,temperature_2m_max,temperature_2m_min&temperature_unit=celsius&timezone=auto`;
      const weatherResponse = await fetch(weatherUrl);

      if (!weatherResponse.ok) {
        throw new Error(`Weather fetch failed: ${weatherResponse.status}`);
      }

      const weatherDataFull = await weatherResponse.json();

      // Log the full response for debugging
      console.log("Weather API response:", weatherDataFull);

      // Extract the current weather and 3-day forecast (today + next 2 days)
      const today = new Date().toISOString().split('T')[0];
      const dailyData = weatherDataFull.daily?.time?.map((time: string, i: number) => ({
        date: time,
        temperature_max: weatherDataFull.daily.temperature_2m_max[i],
        temperature_min: weatherDataFull.daily.temperature_2m_min[i],
        weather_code: weatherDataFull.daily.weather_code[i],
      })) || [];

      // Filter to get today and next 2 days only
      const forecastDays = dailyData.filter(day => day.date >= today).slice(0, 3);

      const weatherData: WeatherData = {
        current: {
          temperature: weatherDataFull.current?.temperature_2m || 0,
          code: weatherDataFull.current?.weather_code || 0,
          is_day: weatherDataFull.current?.is_day || 1,
        },
        daily: forecastDays,
      };

      setWeather(weatherData);

      // Build display name: "City, Country" or "City, State, Country"
      const locationParts = [name];
      if (admin1 && admin1 !== name) locationParts.push(admin1);
      if (country) locationParts.push(country);

      setLocation(locationParts.join(", "));
    } catch (e) {
      console.error("Failed to fetch weather:", e);
      setError(e instanceof Error ? e.message : "Failed to load weather");
    } finally {
      setLoading(false);
    }
  };

  const handleSaveLocation = async () => {
    const newLocation = locationInput().trim();
    if (!newLocation) return;

    setLocation(newLocation);
    setIsEditing(false);

    const updates: Partial<Widget> = {
      data: {
        type: "weather",
        location: newLocation,
        units: units(),
      },
      lastTouched: new Date(),
    };
    props.onUpdate(updates);

    await fetchWeather();
  };

  const toggleUnits = async () => {
    const newUnits = units() === "celsius" ? "fahrenheit" : "celsius";
    setUnits(newUnits);

    const updates: Partial<Widget> = {
      data: {
        type: "weather",
        location: location(),
        units: newUnits,
      },
      lastTouched: new Date(),
    };
    props.onUpdate(updates);
  };

  const getTemperature = () => {
    if (!weather()) return "--";

    const tempC = weather()!.current.temperature;
    return units() === "celsius" ? Math.round(tempC) : Math.round(celsiusToFahrenheit(tempC));
  };

  const getUnitSymbol = () => units() === "celsius" ? "°C" : "°F";

  const getWeatherInfo = () => {
    if (!weather()) return { icon: "🌡", label: "Loading..." };

    const code = weather()!.current.code;
    return WIDGET_CODES[code] || { icon: "🌡", label: "Unknown" };
  };

  const getDayName = (dateStr: string) => {
    const date = new Date(dateStr);
    const today = new Date();
    const tomorrow = new Date(today);
    tomorrow.setDate(tomorrow.getDate() + 1);

    if (date.toDateString() === today.toDateString()) return "Today";
    if (date.toDateString() === tomorrow.toDateString()) return "Tomorrow";

    return date.toLocaleDateString("en-US", { weekday: "short" });
  };

  const getForecastWeatherInfo = (code: number) => {
    return WIDGET_CODES[code] || { icon: "🌡", label: "" };
  };

  return (
    <div class="weather-widget">
      <Show when={isEditing()}>
        <div class="weather-edit">
          <input
            type="text"
            class="weather-location-input"
            placeholder="Enter city name..."
            value={locationInput()}
            onInput={(e) => setLocationInput(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleSaveLocation();
              if (e.key === "Escape") {
                setIsEditing(false);
                setLocationInput(location());
              }
            }}
            autoFocus
          />
          <div class="weather-suggestions">
            <span class="weather-suggestions-label">Popular:</span>
            <For each={POPULAR_CITIES.slice(0, 10)}>
              {(city) => (
                <button
                  class="weather-suggestion-btn"
                  onClick={() => {
                    setLocationInput(city);
                    handleSaveLocation();
                  }}
                >
                  {city}
                </button>
              )}
            </For>
          </div>
          <div class="weather-edit-actions">
            <button class="weather-edit-btn" onClick={handleSaveLocation}>
              Save
            </button>
            <button class="weather-edit-btn weather-edit-cancel" onClick={() => {
              setIsEditing(false);
              setLocationInput(location());
            }}>
              Cancel
            </button>
          </div>
        </div>
      </Show>

      <Show when={!isEditing()}>
        <div class="weather-display">
          <Show when={error()}>
            <div class="weather-error">
              <span class="weather-error-icon">⚠️</span>
              <span class="weather-error-text">{error()}</span>
            </div>
          </Show>

          <Show when={!error()}>
            <div class="weather-current">
              <div class="weather-icon" title={getWeatherInfo().label}>
                {getWeatherInfo().icon}
              </div>

              <div class="weather-temperature">
                <Show when={loading() && !weather()}>
                  <span class="weather-loading">...</span>
                </Show>
                <Show when={!loading() || weather()}>
                  <span class="weather-temp">{getTemperature()}</span>
                  <span class="weather-unit">{getUnitSymbol()}</span>
                </Show>
              </div>
            </div>

            <div class="weather-condition">
              {getWeatherInfo().label}
            </div>

            <div class="weather-location" onClick={() => setIsEditing(true)} title="Click to change location">
              {loading() && !weather() ? "Loading..." : location()}
            </div>

            {/* 3-Day Forecast */}
            <Show when={weather()?.daily && weather()!.daily!.length > 0}>
              <div class="weather-forecast">
                <For each={weather()!.daily}>
                  {(day) => {
                    const info = getForecastWeatherInfo(day.weather_code);
                    return (
                      <div class="forecast-day">
                        <div class="forecast-day-name">{getDayName(day.date)}</div>
                        <div class="forecast-icon">{info.icon}</div>
                        <div class="forecast-temps">
                          <span class="forecast-max">
                            {units() === "celsius"
                              ? Math.round(day.temperature_max)
                              : Math.round(celsiusToFahrenheit(day.temperature_max))
                            }°
                          </span>
                          <span class="forecast-min">
                            {units() === "celsius"
                              ? Math.round(day.temperature_min)
                              : Math.round(celsiusToFahrenheit(day.temperature_min))
                            }°
                          </span>
                        </div>
                      </div>
                    );
                  }}
                </For>
              </div>
            </Show>
          </Show>
        </div>

        <div class="weather-actions">
          <button class="weather-unit-btn" onClick={toggleUnits} title="Toggle units">
            {units() === "celsius" ? "°C" : "°F"}
          </button>
          <button class="weather-refresh-btn" onClick={fetchWeather} title="Refresh" disabled={loading()}>
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none" class={loading() ? "spinning" : ""}>
              <path
                d="M6 2v4M6 8l2-2M6 6l2 2M6 2a4 4 0 014 4"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </button>
        </div>
      </Show>
    </div>
  );
};

export default WeatherWidget;
