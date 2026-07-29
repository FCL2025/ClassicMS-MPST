import { useEffect, useId, useState, type KeyboardEvent } from "react";
import { searchNodes, type SearchHit } from "../api";

interface SearchBarProps {
  /** 目前選取節點的路徑；非空時只搜該節點內。 */
  scope: string | null;
  onClearScope: () => void;
  onPick: (path: string) => void;
}

export function SearchBar({ scope, onClearScope, onPick }: SearchBarProps) {
  const listboxId = useId();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchHit[]>([]);
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
      searchNodes(normalizedQuery, scope)
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
  }, [query, scope]);

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
      if (result) pickResult(result.path);
    }
  };

  return (
    <div className="tree-search">
      {scope ? (
        <div className="tree-search-scope">
          <span className="tree-search-scope-label" title={scope}>
            範圍：{scope}
          </span>
          <button
            type="button"
            className="tree-search-scope-clear"
            aria-label="清除範圍，改為全域搜尋"
            title="清除範圍，改為全域搜尋"
            onClick={onClearScope}
          >
            ×
          </button>
        </div>
      ) : null}
      <div className="tree-search-field">
        <span aria-hidden="true">⌕</span>
        <input
          type="search"
          role="combobox"
          aria-label="搜尋資產路徑或名稱"
          aria-autocomplete="list"
          aria-controls={listboxId}
          aria-expanded={open}
          aria-activedescendant={
            open && results.length > 0 ? `${listboxId}-${activeIndex}` : undefined
          }
          placeholder="搜尋節點、代碼或道具名稱…"
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
                const segments = result.path.split("/");
                const name = segments.pop() ?? result.path;
                return (
                  <button
                    id={`${listboxId}-${index}`}
                    className={index === activeIndex ? "is-active" : ""}
                    key={result.path}
                    type="button"
                    role="option"
                    aria-selected={index === activeIndex}
                    onMouseEnter={() => setActiveIndex(index)}
                    onMouseDown={(event) => {
                      event.preventDefault();
                      pickResult(result.path);
                    }}
                  >
                    <strong>{result.name ?? name}</strong>
                    <span>
                      {result.name ? result.path : segments.join("/") || "根目錄"}
                    </span>
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
