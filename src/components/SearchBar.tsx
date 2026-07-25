import { useEffect, useId, useState, type KeyboardEvent } from "react";
import { searchNodes } from "../api";

interface SearchBarProps {
  onPick: (path: string) => void;
}

export function SearchBar({ onPick }: SearchBarProps) {
  const listboxId = useId();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<string[]>([]);
  const [open, setOpen] = useState(false);
  const [searching, setSearching] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);

  useEffect(() => {
    const normalizedQuery = query.trim();
    if (!normalizedQuery) {
      setResults([]);
      setOpen(false);
      setSearching(false);
      return;
    }

    let active = true;
    setSearching(true);
    const timer = window.setTimeout(() => {
      searchNodes(normalizedQuery)
        .then((nextResults) => {
          if (!active) return;
          setResults(nextResults);
          setActiveIndex(0);
          setOpen(true);
        })
        .catch(() => {
          if (active) setResults([]);
        })
        .finally(() => {
          if (active) setSearching(false);
        });
    }, 150);

    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [query]);

  const pickResult = (path: string) => {
    onPick(path);
    setOpen(false);
  };

  const onInputKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      setOpen(false);
      return;
    }
    if (!open || results.length === 0) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const delta = event.key === "ArrowDown" ? 1 : -1;
      setActiveIndex((current) => (current + delta + results.length) % results.length);
    } else if (event.key === "Enter") {
      event.preventDefault();
      const result = results[activeIndex];
      if (result) pickResult(result);
    }
  };

  return (
    <div className="tree-search">
      <div className="tree-search-field">
        <span aria-hidden="true">⌕</span>
        <input
          type="search"
          role="combobox"
          aria-label="搜尋資產路徑"
          aria-autocomplete="list"
          aria-controls={listboxId}
          aria-expanded={open}
          aria-activedescendant={
            open && results.length > 0 ? `${listboxId}-${activeIndex}` : undefined
          }
          placeholder="搜尋節點或資產路徑…"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onFocus={() => query.trim() && setOpen(true)}
          onBlur={() => window.setTimeout(() => setOpen(false), 120)}
          onKeyDown={onInputKeyDown}
        />
        {searching ? <span className="tree-search-spinner" aria-label="搜尋中" /> : null}
        {query ? (
          <button
            className="tree-search-clear"
            type="button"
            aria-label="清除搜尋"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => setQuery("")}
          >
            ×
          </button>
        ) : null}
      </div>

      {open ? (
        <div className="tree-search-popover">
          <div className="tree-search-result-summary">
            {searching
              ? "搜尋中…"
              : results.length > 0
                ? `${results.length}${results.length >= 200 ? "+" : ""} 筆結果`
                : "找不到符合的資產"}
          </div>
          {results.length > 0 ? (
            <div id={listboxId} className="tree-search-results" role="listbox">
              {results.map((result, index) => {
                const segments = result.split("/");
                const name = segments.pop() ?? result;
                return (
                  <button
                    id={`${listboxId}-${index}`}
                    className={index === activeIndex ? "is-active" : ""}
                    key={result}
                    type="button"
                    role="option"
                    aria-selected={index === activeIndex}
                    onMouseEnter={() => setActiveIndex(index)}
                    onMouseDown={(event) => {
                      event.preventDefault();
                      pickResult(result);
                    }}
                  >
                    <strong>{name}</strong>
                    <span>{segments.join("/") || "根目錄"}</span>
                  </button>
                );
              })}
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
