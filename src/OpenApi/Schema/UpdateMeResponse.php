<?php

declare(strict_types=1);

namespace App\OpenApi\Schema;

use OpenApi\Attributes as OA;

/**
 * OpenAPI 컴포넌트 — 본인 정보 수정 응답.
 *
 * MeResponse 와 같은 프로필에, 비밀번호를 변경한 경우에만 새 토큰 쌍(`tokens`)이 추가된다.
 */
#[OA\Schema(
    schema: 'UpdateMeResponse',
    description: '수정 후 사용자 프로필. 비밀번호 변경 시 다른 기기 세션은 끊기고 현재 기기용 새 토큰 쌍이 `data.tokens` 로 반환된다.',
    allOf: [
        new OA\Schema(ref: '#/components/schemas/MeResponse'),
        new OA\Schema(
            properties: [
                new OA\Property(
                    property: 'data',
                    type: 'object',
                    properties: [
                        new OA\Property(
                            property: 'tokens',
                            type: 'object',
                            description: '비밀번호를 변경한 경우에만 포함 — 기존 토큰은 폐기되므로 이 값으로 교체한다',
                            properties: [
                                new OA\Property(property: 'access_token', type: 'string'),
                                new OA\Property(property: 'token_type', type: 'string', example: 'Bearer'),
                                new OA\Property(property: 'expires_in', type: 'integer', example: 900),
                                new OA\Property(property: 'refresh_token', type: 'string'),
                            ],
                        ),
                    ],
                ),
            ],
        ),
    ],
)]
final class UpdateMeResponse
{
}
