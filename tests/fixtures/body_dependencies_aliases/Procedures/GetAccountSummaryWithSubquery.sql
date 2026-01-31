-- Procedure with subquery aliases - tests nested alias resolution
-- The subquery alias AccountTags should NOT appear in BodyDependencies
CREATE PROCEDURE [dbo].[GetAccountSummaryWithSubquery]
AS
BEGIN
    SET NOCOUNT ON;

    SELECT
        A.Id,
        A.AccountNumber,
        AccountTags.TagCount,
        AccountTags.TagList
    FROM
        [dbo].[Account] A
        LEFT JOIN (
            SELECT
                AT.AccountId,
                COUNT(*) AS TagCount,
                STRING_AGG(T.Name, ', ') AS TagList
            FROM
                [dbo].[AccountTag] AT
                INNER JOIN [dbo].[Tag] T ON T.Id = AT.TagId
            GROUP BY
                AT.AccountId
        ) AS AccountTags ON AccountTags.AccountId = A.Id;
END;
GO
