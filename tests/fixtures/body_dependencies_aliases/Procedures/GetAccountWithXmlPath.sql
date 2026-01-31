-- Complex procedure with STUFF/FOR XML PATH pattern
-- The aliases ITTAG, A, AT should NOT appear in BodyDependencies as schema references
CREATE PROCEDURE [dbo].[GetAccountWithXmlPath]
AS
BEGIN
    SET NOCOUNT ON;

    SELECT
        A.Id AS AccountBusinessKey,
        COUNT(*) AS Occurrences,
        TagDetails.TagList,
        TagDetails.TagCount
    FROM
        [dbo].[Account] A
        LEFT JOIN (
            SELECT
                AccountTags.AccountId,
                STUFF((
                    SELECT ', ' + [ATTAG].[Name]
                    FROM [dbo].[AccountTag] [AT]
                    INNER JOIN [dbo].[Tag] [ATTAG] ON [AT].TagId = [ATTAG].Id
                    WHERE AccountTags.AccountId = [AT].AccountId
                    FOR XML PATH('')
                ), 1, 1, '') AS TagList,
                COUNT(*) AS TagCount
            FROM
                [dbo].[AccountTag] AccountTags
                INNER JOIN [dbo].[Tag] [TAG] ON AccountTags.TagId = [TAG].Id
            GROUP BY
                AccountTags.AccountId
        ) AS TagDetails ON TagDetails.AccountId = A.Id
    GROUP BY
        A.Id,
        TagDetails.TagList,
        TagDetails.TagCount
    HAVING
        COUNT(*) > 1;
END;
GO
