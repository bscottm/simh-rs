;;; pkg-init.el --- LSP and Rust Package Initialization/Customization -*- lexical-binding: t; -*-

;; Package initialization and customizations:

(defun my-split-window-sensibly (&optional window)
  "Split WINDOW vertically if possible, otherwise horizontally."
  (let ((window (or window (selected-window))))
    (or (and (window-splittable-p window t)
             (with-selected-window window (split-window-right)))
        (and (window-splittable-p window)
             (with-selected-window window (split-window-below))))))

(setq split-window-preferred-function #'my-split-window-sensibly)

;; --- "straight.el" bootstrap (should only run once...) ---
(defvar bootstrap-version)
(let ((bootstrap-file
       (expand-file-name
        "straight/repos/straight.el/bootstrap.el"
        (or (bound-and-true-p straight-base-dir)
            user-emacs-directory)))
      (bootstrap-version 7))
  (unless (file-exists-p bootstrap-file)
    (with-current-buffer
        (url-retrieve-synchronously
         "https://raw.githubusercontent.com/radian-software/straight.el/develop/install.el"
         'silent 'inhibit-cookies)
      (goto-char (point-max))
      (eval-print-last-sexp)))
  (load bootstrap-file nil 'nomessage))

;; Integrate straight.el with use-package
(setq straight-use-package-by-default t)
(straight-use-package 'use-package)

;; --- Org mode ---
(straight-use-package 'org)

;; --- Vertico: The Vertical Minibuffer UI ---
(use-package vertico
  :straight t
  :init
  (vertico-mode 1)
  :custom
  (vertico-cycle t)) ; Wrap around when you reach the bottom

;; --- COMPLETION & FILTERING (Corfu + Marginalia + Orderless) ---
;; (Marginalia: Rich metadata in the margin)
(use-package orderless
  :custom
  (completion-styles '(orderless basic))
  (completion-category-overrides '((file (styles basic partial-completion)))))

(use-package marginalia
  :straight t
  :init
  (marginalia-mode 1))

(use-package corfu
  :init
  (global-corfu-mode)
  :custom
  (corfu-auto t)
  (corfu-auto-delay 0.1)
  (corfu-auto-prefix 2)
  (corfu-quit-at-boundary nil)
  (corfu-separator ?\s)
  (global-corfu-minibuffer nil)
  :bind (:map corfu-map
              ("TAB" . corfu-insert)
              ("C-n" . corfu-next)
              ("C-p" . corfu-previous))
  :config
  (add-hook 'minibuffer-setup-hook (lambda () (corfu-mode -1)))
  (corfu-popupinfo-mode 1)
  (setq corfu-popupinfo-delay 0.5))

;; --- Consult ---
(use-package consult
  :straight t
  :bind (("C-s" . consult-line)             ;; Better isearch
         ("M-y" . consult-yank-pop)         ;; Better kill-ring
         ("C-c q" . consult-ripgrep)        ;; Fast project search
         ("M-g i" . consult-imenu)          ;; Jump to any symbol in the current buffer
         ("M-g I" . consult-imenu-multi)    ;; Jump to any symbol in the entire PROJECT
         ("M-g o" . consult-outline)))      ;; Outline

;; --- Embark: The "Right-Click" Keyboard Shortcut ---
(use-package embark
  :straight t
  :bind (("C-." . embark-act)         ; Pick an action for what's at point
         ("M-." . embark-dwim))       ; "Do What I Mean"
  :init
  (setq prefix-help-command #'embark-prefix-help-command))

;; --- HOVER DOCUMENTATION (Eldoc-Box) ---
(use-package eldoc-box
  :straight t)

;; --- SNIPPETS & GIT (Yasnippet + Magit) ---
(use-package yasnippet
  :init (yas-global-mode 1))

;; --- FORMATTING (Markdown Mode) ---
(use-package markdown-mode
  :straight t)

;; --- Shared Rust mode configuration ---
(defun my/rust-mode-setup ()
  "Common setup for both rustic-mode and rust-ts-mode."
  (require 'whitespace)
  (setq-local display-fill-column-indicator-column 110)
  (display-fill-column-indicator-mode 1)
  (setq-local whitespace-line-column 132
              whitespace-style '(face lines-tail)
              fill-column 110
              tab-width 4
              c-basic-offset 4
              indent-tabs-mode nil)
  (whitespace-mode 1)
  (add-hook 'before-save-hook #'delete-trailing-whitespace nil t))

;; --- RUST CORE (Rustic + Eglot + Flymake + Tree-Sitter + LSP) ---
;; (use-package eglot
;;   :straight nil ; Built-in
;;   :hook ((rustic-mode . eglot-ensure)
;;          (rust-ts-mode . eglot-ensure)
;;          (c-mode . eglot-ensure)
;;          (c++-mode . eglot-ensure))
;;   :bind (:map eglot-mode-map
;;               ("C-c h" . eldoc-box-help-at-point)
;;               ("C-c d" . eldoc))
;;   :config
;;   (setq eglot-send-changes-idle-time 0.1)
;;   (setq eglot-workspace-configuration
;;         '((rust-analyzer . ((trace.server . "off")))))
;;   (setq eglot-keep-traces nil)
;;   (setq eglot-events-buffer-config '(:size 0))
;;   (add-hook 'eglot-managed-mode-hook #'eglot-inlay-hints-mode)
;;   (declare-function eglot-inlay-hints-mode "eglot")
;;   (add-to-list 'eglot-server-programs
;;                `(rustic-mode . ("rust-analyzer" :initializationOptions
;;                                 (:check (:command "clippy")))))
;;   (add-to-list 'eglot-server-programs
;;                `(rust-ts-mode . ("rust-analyzer" :initializationOptions
;;                                  (:check (:command "clippy"))))))

(use-package lsp-mode
  :hook ((rustic-mode . lsp)
         (rust-ts-mode . lsp)
         (c-mode . lsp)
         (c++-mode . lsp))
  :commands lsp
  :config
  (setq lsp-auto-guess-root t)
  (setq lsp-idle-delay 0.1)  ; equivalent to eglot-send-changes-idle-time
  (setq lsp-log-io nil)      ; reduces trace output, similar to eglot-keep-traces
  (setq lsp-print-io nil)    ; suppresses event buffer spam
  (setq lsp-headerline-breadcrumb-enable nil)
  (setq lsp-eldoc-enable-hover nil) ; disable lsp hover since you use eldoc-box
  (lsp-register-custom-settings
   '(("rust-analyzer.trace.server" "off")))
  (add-hook 'lsp-mode-hook #'lsp-lens-mode) ; enables inlay hints if desired
  (setq lsp-rust-analyzer-cargo-watch-command "clippy")
  (setq lsp-rust-analyzer-proc-macro-enable t)
   :bind (:map lsp-mode-map
               ("C-c h" . eldoc-box-help-at-point)
               ("C-c d" . eldoc)))

;; Conflicting completions with Corfu:

;; (use-package company
;;   :straight t
;;   :after lsp-mode
;;   :config
;;   (global-company-mode))

;; (use-package company-lsp
;;   :straight t
;;   :after (lsp-mode company)
;;   :config
;;   (push 'company-lsp company-backends))   

(use-package rust-mode
  :ensure t
  :straight t
  :init
  (setq rust-mode-treesitter-derive t)
  :hook (rust-mode . my/rust-mode-setup))

(setq treesit-language-source-alist
      (append
       (if (<= emacs-major-version 30)
           ;; TreeSitter ABI 14 for Emacs 30 and earlier:
           '((rust . ("https://github.com/tree-sitter/tree-sitter-rust" "v0.21.2"))
             (c . ("https://github.com/tree-sitter/tree-sitter-c" "v0.23.6"))
             (cpp . ("https://github.com/tree-sitter/tree-sitter-cpp" "v0.23.4")))
         ;; And ABI 15+ for Emacs 31 and later which originates from the master branch.
         '((rust . ("https://github.com/tree-sitter/tree-sitter-rust"))
           (c . ("https://github.com/tree-sitter/tree-sitter-c"))
           (cpp . ("https://github.com/tree-sitter/tree-sitter-cpp"))))
       ;; Ordinary languages that don't need versioning. Yet.
       '((toml . ("https://github.com/tree-sitter/tree-sitter-toml")))
       '((json . ("https://github.com/tree-sitter/tree-sitter-json")))))

(use-package treesit-auto
  :straight t
  :custom
  (treesit-auto-install 'prompt)
  :config
  ;; Remove rust from treesit-auto so rustic-mode can manage it
  (setq treesit-auto-langs (delete 'rust treesit-auto-langs))
  (treesit-auto-add-to-auto-mode-alist 'all)
  (global-treesit-auto-mode))

(use-package flymake
  :straight nil
  :config
  (setq flymake-error-bitmap '(exclamation-mark flymake-error-fringe))
  (setq flymake-warning-bitmap '(exclamation-mark flymake-warning-fringe))
  (setq flymake-note-bitmap '(exclamation-mark flymake-note-fringe))
  :bind (:map flymake-mode-map
              ("M-g n" . flymake-goto-next-error)
              ("M-g p" . flymake-goto-prev-error)))

(use-package inheritenv
  :straight t
  :ensure t)

(use-package rustic
  :straight t
  ; Ensure Rust, LSP modes and treesit are loaded before Rustic!
  :after (rust-mode lsp-mode treesit inheritenv flymake)
  :bind (:map rustic-mode-map
              ("M-j" . lsp-ui-imenu)
              ("M-?" . lsp-find-references)
              ("C-c C-c b" . rustic-cargo-build)
              ("C-c C-c c" . rustic-cargo-clean)
              ("C-c C-c l" . flymake-show-buffer-diagnostics)
              ("C-c C-c s" . rustic-cargo-spellcheck)
              ("C-c C-c a" . eglot-code-actions)
              ("C-c C-c r" . eglot-rename))
  :preface
  ;; This tells rustic to define the TS functions before it initializes
  (setq rustic-tree-sitter t) 
  :config
  (setq rustic-mode-ts-inferior-mode 'rust-ts-mode)
  (setq rustic-lsp-client 'eglot)
  (setq rustic-format-on-save t)
  ;; Emacs 30 font lock level
  (setq treesit-font-lock-level 4)
  :hook (rustic-mode . my/rust-mode-setup))

;; --- Integration glue: Consult-Eglot + Embark-consult
(use-package consult-eglot
  :straight t
  :after (eglot consult)
  :bind (:map rustic-mode-map
              ("C-c C-s" . consult-eglot-symbols)))

(use-package embark-consult
  :straight t
  :after (embark consult))

;; --- Savehist: Persist minibuffer history ---
(use-package savehist
  :straight nil ; Built-in
  :init
  (savehist-mode 1))

;; --- Minimap: The VS Code bird's-eye view ---
(use-package minimap
  :straight t
  :custom
  (minimap-window-location 'right)
  (minimap-update-delay 0.1)
  (minimap-width-fraction 0.1)
  :bind ("C-c m" . minimap-mode))

;; --- Treemacs: The File Explorer Sidebar ---
(defun my/treemacs-mode-setup ()
  "Setup for treemacs"
  (buffer-face-set '(:height 100))
  (setq treemacs-git-mode nil)
  (treemacs-git-mode nil))

(use-package treemacs
  :straight t
  :bind ("C-c t" . treemacs)
  :config
  ;; Disable git mode completely
  (setq treemacs-git-mode nil)
  ;; Ensure git integration is not enabled via other variables
  (setq treemacs-git-integration nil)
  ;; Toggle it off manually if you want other filewatch features
  (treemacs-filewatch-mode -1)
  :hook
  (treemacs-mode . my/treemacs-mode-setup))

(use-package treemacs-icons-dired
  :straight t
  :after treemacs dired
  :ensure t
  :config (treemacs-icons-dired-mode))

(use-package treemacs-all-the-icons
  :straight t
  ;; :config (setq treemacs-theme 'all-the-icons))
  :config (treemacs-load-theme "all-the-icons"))

;; NOTE: all-the-icons-install-fonts
;;
;; Windows: Download somewhere, then right click on each font to install.

;; --- GIT INTEGRATION (Magit) ---
(use-package magit
  :bind ("C-x g" . magit-status)
  :config
  ;; Windows Performance: Avoid slow status refreshes on large repos
  (setq magit-refresh-status-buffer nil)
  (remove-hook 'server-switch-hook 'magit-commit-diff)
  (setq magit-commit-show-diff nil))

;; --- PROJECT NAVIGATION (project.el) ---
(use-package project
  :straight nil ; It is built-in
  :bind (("C-x p f" . project-find-file)
         ("C-x p p" . project-switch-project)
         ("C-x p v" . magit-project-status)
         ("C-x p s" . consult-ripgrep)))

;; --- Veteran Preferences ---
(global-set-key (kbd "C-/") 'comment-line) ; Modern toggle

;; --- "Sensible" window splitting ---
(setq split-height-threshold 120
      split-width-threshold 160)
;; (setq split-height-threshold nil          ; Force horizontal splits
;;      split-width-threshold 0)

;; --- Extra Rustic workspace-related commands ---
(defun my/rustic-cargo-build-workspace ()
  "Build the entire workspace from root"
  (interactive)
  (let ((rustic-compile-directory-method 'rustic-buffer-workspace-root))
    (rustic-cargo-build)))

(defun my/rustic-cargo-clean-workspace ()
  "Clean the entire workspace from root"
  (interactive)
  (let ((rustic-compile-directory-method 'rustic-buffer-workspace-root))
    (rustic-cargo-clean)))

;; Bind to keys
(with-eval-after-load 'rustic
  (define-key rustic-mode-map (kbd "C-c C-c w b") #'my/rustic-cargo-build-workspace)
  (define-key rustic-mode-map (kbd "C-c C-c w c") #'my/rustic-cargo-clean-workspace))

(with-eval-after-load 'rust-ts-mode
  (require 'rustic)
  (define-key rust-ts-mode-map (kbd "C-c C-c") (lookup-key rustic-mode-map (kbd "C-c C-c")))
  (define-key rust-ts-mode-map (kbd "C-c C-c w b") #'my/rustic-cargo-build-workspace)
  (define-key rust-ts-mode-map (kbd "C-c C-c w c") #'my/rustic-cargo-clean-workspace))

;; --- CMake foo ---
;; (use-package project-cmake
;;   :straight (:host github :repo "lucius-martius/project-cmake")
;;   ;; This tells Emacs: "If you see this function, load the package"
;;   :commands (project-cmake-find-root)
;;   :init
;;   (with-eval-after-load 'project
;;     (add-hook 'project-find-functions #'project-cmake-find-root -1)))

;; (with-eval-after-load 'project
;;   (add-hook 'project-find-functions #'my/project-try-cmake))
;; (defun my/project-try-cmake (dir)
;;   "Identify a CMake project root for project.el safely."
;;   (let ((root (locate-dominating-file dir "CMakeLists.txt")))
;;     (when root
;;       ;; Returning a cons cell (backend . root-string)
;;       ;; is the most compatible format for Emacs 29+
;;       (cons 'transient root))))

;; (with-eval-after-load 'project
;;   (add-hook 'project-find-functions #'my/project-try-cmake))

(setq project-vc-extra-root-markers '("CMakeLists.txt"))

(use-package rainbow-delimiters
  :straight t
  :hook ((prog-mode . rainbow-delimiters-mode)))

;; Rainbow modes:
;; (require 'rainbow-blocks)
;; (add-hook 'lisp-mode-hook 'rainbow-blocks-mode)

(provide 'pkg-init)
