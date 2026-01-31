-- Procedure with table aliases
-- Tests that aliases like A, ATTAG are NOT included in BodyDependencies
CREATE PROCEDURE [dbo].[GetAccountTags]
    @AccountId INT
AS
BEGIN
    SET NOCOUNT ON;

    SELECT
        A.Id AS AccountId,
        A.AccountNumber,
        ATTAG.Name AS TagName
    FROM
        [dbo].[Account] A
        INNER JOIN [dbo].[AccountTag] AT ON AT.AccountId = A.Id
        INNER JOIN [dbo].[Tag] ATTAG ON ATTAG.Id = AT.TagId
    WHERE
        A.Id = @AccountId
    ORDER BY
        ATTAG.Name;
END;
GO
