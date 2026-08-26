<?php

declare(strict_types=1);

namespace Envy;

final class EnvyException extends \RuntimeException
{
    /**
     * @param list<string> $problems every problem found in a single pass
     */
    public function __construct(
        public readonly array $problems,
    ) {
        parent::__construct(
            'envy configuration invalid (' . count($problems) . " problem(s)):\n  - "
            . implode("\n  - ", $problems)
        );
    }
}
