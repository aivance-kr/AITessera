<?php

declare(strict_types=1);

namespace App\Domain;

/**
 * 본인 정보 수정 결과 — 수정 후 프로필 + (비밀번호 변경 시) 새로 발급한 토큰 쌍.
 *
 * 비밀번호를 바꾸면 다른 기기의 세션을 끊기 위해 리프레시 토큰을 모두 폐기하므로,
 * 현재 기기가 세션을 이어갈 수 있도록 새 토큰 쌍을 함께 돌려준다.
 */
final readonly class UpdateMeResult
{
    public function __construct(
        public UserProfile $profile,
        public ?TokenPair $tokens = null,
    ) {
    }

    /**
     * @return array<string, mixed>
     */
    public function toArray(): array
    {
        $data = $this->profile->toArray();
        if ($this->tokens !== null) {
            $data['tokens'] = $this->tokens->toArray();
        }

        return $data;
    }
}
