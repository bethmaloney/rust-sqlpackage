-- Procedure with INSERT...SELECT containing aliases
-- Tests alias resolution in INSERT statements with SELECT
CREATE PROCEDURE [dbo].[InsertSelectWithAliases]
    @SourceAccountId INT,
    @TargetAccountId INT
AS
BEGIN
    SET NOCOUNT ON;

    -- Copy tags from one account to another
    INSERT INTO [dbo].[AccountTag] (AccountId, TagId)
    SELECT
        @TargetAccountId AS AccountId,
        [AT1].TagId
    FROM [dbo].[AccountTag] [AT1]
    INNER JOIN [dbo].[Tag] [T1] ON [T1].Id = [AT1].TagId
    WHERE [AT1].AccountId = @SourceAccountId
        AND NOT EXISTS (
            SELECT 1
            FROM [dbo].[AccountTag] [AT2]
            WHERE [AT2].AccountId = @TargetAccountId
                AND [AT2].TagId = [AT1].TagId
        );
END;
GO
