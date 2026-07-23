import { forwardRef } from "react";
import { Button as SitoButton } from "@sito/ui";

import { classNames } from "@/shared/utils";

import "@/styles/components/Button.css";

import type { ButtonProps } from "./types";

const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ type = "button", unstyled, className, ...props }, ref) => {
    const sharedProps = {
      ...props,
      ref,
      type,
      className: classNames(!unstyled && "Button", className),
    };

    // Bespoke controls opt out of every base visual class. @sito/ui intentionally always applies
    // its own class stack, so preserve this repo's `unstyled` contract with the native element.
    if (unstyled) return <button {...sharedProps} />;

    return <SitoButton {...sharedProps} />;
  },
);

export default Button;
