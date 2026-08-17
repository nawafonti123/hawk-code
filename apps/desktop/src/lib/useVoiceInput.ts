import { useCallback, useEffect, useRef, useState } from "react";

interface SpeechAlternativeLike {
  transcript: string;
}

interface SpeechResultLike {
  readonly isFinal: boolean;
  readonly length: number;
  readonly [index: number]: SpeechAlternativeLike;
}

interface SpeechResultListLike {
  readonly length: number;
  readonly [index: number]: SpeechResultLike;
}

interface SpeechEventLike extends Event {
  readonly resultIndex: number;
  readonly results: SpeechResultListLike;
}

interface SpeechErrorEventLike extends Event {
  readonly error: string;
}

interface SpeechRecognitionLike {
  continuous: boolean;
  interimResults: boolean;
  lang: string;
  onresult: ((event: SpeechEventLike) => void) | null;
  onerror: ((event: SpeechErrorEventLike) => void) | null;
  onend: (() => void) | null;
  start(): void;
  stop(): void;
}

interface SpeechRecognitionConstructor {
  new (): SpeechRecognitionLike;
}

declare global {
  interface Window {
    SpeechRecognition?: SpeechRecognitionConstructor;
    webkitSpeechRecognition?: SpeechRecognitionConstructor;
  }
}

interface VoiceInputOptions {
  language: string;
  currentText: string;
  onTranscript: (text: string) => void;
  onError: (message: string) => void;
}

const BAR_COUNT = 18;

export function useVoiceInput({
  language,
  currentText,
  onTranscript,
  onError,
}: VoiceInputOptions) {
  const [recording, setRecording] = useState(false);
  const [levels, setLevels] = useState<number[]>(() =>
    Array.from({ length: BAR_COUNT }, () => 0.12),
  );
  const activeRef = useRef(false);
  const baseTextRef = useRef("");
  const finalTextRef = useRef("");
  const recognitionRef = useRef<SpeechRecognitionLike | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const audioContextRef = useRef<AudioContext | null>(null);
  const animationRef = useRef<number | null>(null);

  const releaseAudio = useCallback(() => {
    if (animationRef.current !== null)
      cancelAnimationFrame(animationRef.current);
    animationRef.current = null;
    streamRef.current?.getTracks().forEach((track) => track.stop());
    streamRef.current = null;
    const context = audioContextRef.current;
    audioContextRef.current = null;
    if (context && context.state !== "closed") void context.close();
    setLevels(Array.from({ length: BAR_COUNT }, () => 0.12));
  }, []);

  const stop = useCallback(() => {
    activeRef.current = false;
    try {
      recognitionRef.current?.stop();
    } catch {
      // The recognition session may already be stopping.
    }
    recognitionRef.current = null;
    releaseAudio();
    setRecording(false);
  }, [releaseAudio]);

  const start = useCallback(async () => {
    const Recognition =
      window.SpeechRecognition ?? window.webkitSpeechRecognition;
    if (!Recognition) {
      onError("VOICE_RECOGNITION_UNAVAILABLE");
      return;
    }
    if (!navigator.mediaDevices?.getUserMedia) {
      onError("MICROPHONE_UNAVAILABLE");
      return;
    }

    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          autoGainControl: true,
          echoCancellation: true,
          noiseSuppression: true,
        },
      });
      streamRef.current = stream;
      const context = new AudioContext();
      audioContextRef.current = context;
      const analyser = context.createAnalyser();
      analyser.fftSize = 128;
      analyser.smoothingTimeConstant = 0.74;
      context.createMediaStreamSource(stream).connect(analyser);
      const frequencies = new Uint8Array(analyser.frequencyBinCount);
      let lastPaint = 0;
      const paint = (now: number) => {
        if (!activeRef.current) return;
        analyser.getByteFrequencyData(frequencies);
        if (now - lastPaint > 34) {
          lastPaint = now;
          const next = Array.from({ length: BAR_COUNT }, (_, index) => {
            const startIndex = Math.floor(
              (index / BAR_COUNT) * frequencies.length,
            );
            const endIndex = Math.max(
              startIndex + 1,
              Math.floor(((index + 1) / BAR_COUNT) * frequencies.length),
            );
            let total = 0;
            for (let cursor = startIndex; cursor < endIndex; cursor += 1)
              total += frequencies[cursor] ?? 0;
            const average = total / (endIndex - startIndex);
            return Math.max(0.12, Math.min(1, average / 92));
          });
          setLevels(next);
        }
        animationRef.current = requestAnimationFrame(paint);
      };

      baseTextRef.current = currentText.trimEnd();
      finalTextRef.current = "";
      activeRef.current = true;
      setRecording(true);
      animationRef.current = requestAnimationFrame(paint);

      const recognition = new Recognition();
      recognitionRef.current = recognition;
      recognition.lang = language === "ar" ? "ar-SA" : language === "en" ? "en-US" : language;
      recognition.continuous = true;
      recognition.interimResults = true;
      recognition.onresult = (event) => {
        let interim = "";
        for (
          let index = event.resultIndex;
          index < event.results.length;
          index += 1
        ) {
          const result = event.results[index];
          const transcript = result?.[0]?.transcript ?? "";
          if (result?.isFinal) finalTextRef.current += `${transcript.trim()} `;
          else interim += transcript;
        }
        onTranscript(
          [baseTextRef.current, finalTextRef.current.trim(), interim.trim()]
            .filter(Boolean)
            .join(" "),
        );
      };
      recognition.onerror = (event) => {
        if (event.error !== "aborted" && event.error !== "no-speech")
          onError(`VOICE_ERROR:${event.error}`);
      };
      recognition.onend = () => {
        if (!activeRef.current) return;
        try {
          recognition.start();
        } catch {
          stop();
        }
      };
      recognition.start();
    } catch (error) {
      stop();
      onError(error instanceof Error ? error.message : String(error));
    }
  }, [currentText, language, onError, onTranscript, stop]);

  useEffect(() => stop, [stop]);

  return { levels, recording, start, stop };
}
