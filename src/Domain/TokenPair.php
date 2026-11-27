<?php

declare(strict_types=1);

namespace App\Domain;

/**
 * 발급된 토큰 쌍 — Access(JWT, 단기) + Refresh(불투명 문자열, 장기).
 */
final readonly class TokenPair
{
    public function __construct(
        public string $accessToken,
        public int $expiresIn,
        public string $refreshToken,
    ) {
    }

    /**
     * 표준 토큰 응답 본문(`TokenResponse.data`).
     *
     * @return array{access_token: string, token_type: string, expires_in: int, refresh_token: string}
     */
    public function toArray(): array
    {
        return [
            'access_token' => $this->accessToken,
            'token_type' => 'Bearer',
            'expires_in' => $this->expiresIn,
            'refresh_token' => $this->refreshToken,
        ];
    }
}
